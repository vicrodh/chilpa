// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
// Plasma (immersive shader scene, mode 1) — the QRhi renderer behind
// PlasmaItem. Ports the Slint feedback loop (crates/qbz/src/
// shader_underlay.rs:1003-1025: render the frame sampling the history
// texture, then copy it back) as a PING-PONG over two owned RGBA8 textures
// (the reference's TEX_FORMAT Rgba8Unorm, shader_underlay.rs:39): pass 1
// renders the plasma into history[write] sampling history[read], pass 2
// blits history[write] into the item's render target, then the indices
// swap. Owning both targets means the item's texture is never read back and
// no Qt-internal attachment is touched.
//
// Resource cadence (the pulse law): `synchronize` snapshots the QML-side
// properties when the item updates (pulse tick only); per frame the GPU
// work is one uniform upload + two fullscreen triangles — cheaper than any
// of the ShaderEffect scenes.
//
// History size tracks the item's physical size CAPPED at 2560x1440 (the
// reference's offscreen-target ceiling, shader_underlay.rs:37-39; the
// ShaderSceneLayer resW/resH rule); on a bigger screen the blit upscales.
// A resize rebuilds the pair and re-clears it (the field restarts — the
// same thing the reference gets when its window-size-tracking resources
// rebuild, shader_underlay.rs:522-523).

#include "plasma_item.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QFile>
#include <QtQml/qqml.h>
#include <rhi/qrhi.h>

#include <cstring>

namespace {

// The history-target ceiling (see the header).
constexpr int MAX_W = 2560;
constexpr int MAX_H = 1440;

// The FULL 144-byte std140 scene block (spec 01 §1) — plasma.frag declares
// every field even though it reads only some; the unread ones stay zero.
struct UniformData
{
    float time;        //   0
    float phase;       //   4
    float beat;        //   8
    float level;       //  12
    float res[2];      //  16
    float levelSmooth; //  24
    float transient;   //  28
    float energyLo[4]; //  32
    float energyHi[4]; //  48
    float bandsLo[4];  //  64
    float bandsHi[4];  //  80
    float primary[4];  //  96
    float secondary[4];// 112
    float accent[4];   // 128
};
static_assert(sizeof(UniformData) == 144, "std140 scene block");

QShader loadShader(const QString &qrcPath)
{
    QFile f(qrcPath);
    if (f.open(QIODevice::ReadOnly)) {
        const QShader s = QShader::fromSerialized(f.readAll());
        if (!s.isValid())
            qWarning("[plasma] shader %s failed to deserialize", qPrintable(qrcPath));
        return s;
    }
    qWarning("[plasma] shader %s not found in the qrc", qPrintable(qrcPath));
    return QShader();
}

QVector4D colorVec(const QColor &c)
{
    return QVector4D(float(c.redF()), float(c.greenF()), float(c.blueF()), 1.0f);
}

class PlasmaRenderer : public QQuickRhiItemRenderer
{
public:
    ~PlasmaRenderer() override
    {
        // The render pass descriptors are OURS (the RTs borrow them).
        delete m_histRpDesc[0];
        delete m_histRpDesc[1];
    }

    void initialize(QRhiCommandBuffer *cb) override
    {
        QRhi *r = cb->rhi();
        if (m_resourceOwner && m_resourceOwner != r) {
            m_blitPipe.reset();
            m_plasmaPipe.reset();
            for (int i = 0; i < 2; ++i) {
                m_srbBlit[i].reset();
                m_srbPlasma[i].reset();
                m_histRt[i].reset();
                delete m_histRpDesc[i];
                m_histRpDesc[i] = nullptr;
                m_hist[i].reset();
            }
            m_sampler.reset();
            m_ubuf.reset();
            m_vbuf.reset();
            m_plasmaPassDescriptor = nullptr;
            m_blitPassDescriptor = nullptr;
            m_histSize = QSize();
        }
        m_resourceOwner = r;
        if (m_vbuf)
            return;
        // The fullscreen triangle (rhi_triangle.vert replaces gl_VertexID
        // with this buffer — the GLES 3.0-portable spelling, linebed
        // idiom). Shared by the feedback AND blit passes.
        static const float TRI[] = { -1.0f, -1.0f, 3.0f, -1.0f, -1.0f, 3.0f };
        m_vbuf.reset(r->newBuffer(QRhiBuffer::Immutable,
                                  QRhiBuffer::VertexBuffer,
                                  quint32(sizeof(TRI))));
        m_ubuf.reset(r->newBuffer(QRhiBuffer::Dynamic,
                                  QRhiBuffer::UniformBuffer,
                                  quint32(sizeof(UniformData))));
        m_sampler.reset(r->newSampler(QRhiSampler::Linear,
                                      QRhiSampler::Linear,
                                      QRhiSampler::None,
                                      QRhiSampler::ClampToEdge,
                                      QRhiSampler::ClampToEdge));
        if (!m_vbuf->create() || !m_ubuf->create() || !m_sampler->create()) {
            qWarning("[plasma] resource creation failed — the scene stays dark");
            m_vbuf.reset();
            return;
        }
        QRhiResourceUpdateBatch *u = r->nextResourceUpdateBatch();
        u->uploadStaticBuffer(m_vbuf.get(), TRI);
        cb->resourceUpdate(u);
    }

    void synchronize(QQuickRhiItem *item) override
    {
        // GUI thread: snapshot the item's properties. Runs on item update()
        // (the pulse tick) and on geometry/rhi changes.
        auto *pl = static_cast<PlasmaItem *>(item);
        m_time = pl->m_time;
        m_beat = pl->m_beat;
        m_level = pl->m_level;
        m_levelSmooth = pl->m_levelSmooth;
        m_energyLo = pl->m_energyLo;
        m_energyHi = pl->m_energyHi;
        m_transientAmp = pl->m_transientAmp;
        m_bandsLo = pl->m_bandsLo;
        m_bandsHi = pl->m_bandsHi;
        m_primary = colorVec(pl->m_primary);
        m_secondary = colorVec(pl->m_secondary);
        m_accent = colorVec(pl->m_accent);
    }

    void render(QRhiCommandBuffer *cb) override
    {
        if (!m_vbuf)
            return; // resource creation failed in initialize
        QRhiRenderTarget *rt = renderTarget();
        const QSize ps = rt->pixelSize();
        if (ps.isEmpty())
            return;
        const QSize hist = capped(ps);
        if (hist != m_histSize)
            rebuildHistory(hist);
        if (!m_hist[0])
            return; // history creation failed — stay dark

        // Pipeline A (plasma) renders into the history pair's pass; pipeline
        // B (blit) into the item's. Rebuild each when its descriptor moves.
        if (!m_plasmaPipe || m_histRt[0]->renderPassDescriptor() != m_plasmaPassDescriptor) {
            m_plasmaPassDescriptor = m_histRt[0]->renderPassDescriptor();
            buildPlasmaPipeline();
            if (!m_plasmaPipe)
                return;
        }
        if (!m_blitPipe || rt->renderPassDescriptor() != m_blitPassDescriptor) {
            m_blitPassDescriptor = rt->renderPassDescriptor();
            buildBlitPipeline();
            if (!m_blitPipe)
                return;
        }

        QRhiResourceUpdateBatch *u = rhi()->nextResourceUpdateBatch();
        UniformData ud = {};
        ud.time = m_time;
        ud.beat = m_beat;
        ud.level = m_level;
        ud.res[0] = float(hist.width());
        ud.res[1] = float(hist.height());
        ud.levelSmooth = m_levelSmooth;
        ud.transient = m_transientAmp;
        std::memcpy(ud.energyLo, &m_energyLo, sizeof(ud.energyLo));
        std::memcpy(ud.energyHi, &m_energyHi, sizeof(ud.energyHi));
        std::memcpy(ud.bandsLo, &m_bandsLo, sizeof(ud.bandsLo));
        std::memcpy(ud.bandsHi, &m_bandsHi, sizeof(ud.bandsHi));
        std::memcpy(ud.primary, &m_primary, sizeof(ud.primary));
        std::memcpy(ud.secondary, &m_secondary, sizeof(ud.secondary));
        std::memcpy(ud.accent, &m_accent, sizeof(ud.accent));
        u->updateDynamicBuffer(m_ubuf.get(), 0, quint32(sizeof(ud)), &ud);

        // A fresh pair starts BLACK (the reference's history starts
        // zero-filled too — an unsampled-cleared first frame would advect
        // garbage).
        if (m_needClear) {
            m_needClear = false;
            for (int i = 0; i < 2; ++i) {
                cb->beginPass(m_histRt[i].get(), QColor(0, 0, 0, 255),
                              QRhiDepthStencilClearValue(1.0f, 0), nullptr);
                cb->endPass();
            }
        }

        const int read = m_read;
        const int write = 1 - m_read;

        // Pass 1: plasma into history[write], sampling history[read].
        cb->beginPass(m_histRt[write].get(), QColor(0, 0, 0, 255),
                      QRhiDepthStencilClearValue(1.0f, 0), u);
        cb->setGraphicsPipeline(m_plasmaPipe.get());
        cb->setViewport(QRhiViewport(0, 0, float(hist.width()), float(hist.height())));
        cb->setShaderResources(m_srbPlasma[read].get());
        const QRhiCommandBuffer::VertexInput tri(m_vbuf.get(), 0);
        cb->setVertexInput(0, 1, &tri);
        cb->draw(3);
        cb->endPass();

        // Pass 2: blit history[write] into the item target (the composite).
        cb->beginPass(rt, QColor(0, 0, 0, 255), QRhiDepthStencilClearValue(1.0f, 0), nullptr);
        cb->setGraphicsPipeline(m_blitPipe.get());
        cb->setViewport(QRhiViewport(0, 0, float(ps.width()), float(ps.height())));
        cb->setShaderResources(m_srbBlit[write].get());
        cb->setVertexInput(0, 1, &tri);
        cb->draw(3);
        cb->endPass();

        m_read = write;
    }

private:
    QRhi *m_resourceOwner = nullptr;
    static QSize capped(const QSize &ps)
    {
        if (ps.width() <= MAX_W && ps.height() <= MAX_H)
            return ps;
        const double s = qMin(double(MAX_W) / ps.width(), double(MAX_H) / ps.height());
        return QSize(qMax(1, int(ps.width() * s)), qMax(1, int(ps.height() * s)));
    }

    // (Re)build the ping-pong pair + its render targets + both SRB sets.
    // Old resources go through deleteLater() — an in-flight frame may still
    // reference them (QRhiResource::deleteLater is the documented deferral).
    void rebuildHistory(const QSize &size)
    {
        QRhi *r = rhi();
        for (int i = 0; i < 2; ++i) {
            if (m_hist[i])
                m_hist[i].release()->deleteLater();
            if (m_histRt[i])
                m_histRt[i].release()->deleteLater();
            if (m_srbPlasma[i])
                m_srbPlasma[i].release()->deleteLater();
            if (m_srbBlit[i])
                m_srbBlit[i].release()->deleteLater();
        }
        if (m_plasmaPipe)
            m_plasmaPipe.release()->deleteLater();
        m_plasmaPassDescriptor = nullptr;
        m_histSize = size;
        m_needClear = true;
        m_read = 0;

        for (int i = 0; i < 2; ++i) {
            // These textures are both sampled and rendered into. Vulkan's
            // driver path happened to accept the missing usage bit, while the
            // D3D backend correctly refused the texture render target and left
            // Plasma black on Windows.
            m_hist[i].reset(r->newTexture(QRhiTexture::RGBA8,
                                          size,
                                          1,
                                          QRhiTexture::RenderTarget));
            if (!m_hist[i]->create()) {
                qWarning("[plasma] history texture creation failed — the scene stays dark");
                m_hist[0].reset();
                m_hist[1].reset();
                return;
            }
            // Vulkan/D3D/Metal REQUIRE a render pass descriptor before
            // create(); without it the RT creation fails and the later
            // beginPass corrupts the whole window (2026-08-15 owner smoke:
            // Plasma left a transparent, unusable window). The RT does NOT
            // own the descriptor — free the previous one on rebuild.
            if (m_histRpDesc[i]) {
                m_histRpDesc[i]->deleteLater();
                m_histRpDesc[i] = nullptr;
            }
            m_histRt[i].reset(r->newTextureRenderTarget({ m_hist[i].get() }));
            m_histRpDesc[i] = m_histRt[i]->newCompatibleRenderPassDescriptor();
            m_histRt[i]->setRenderPassDescriptor(m_histRpDesc[i]);
            if (!m_histRt[i]->create()) {
                qWarning("[plasma] history render target failed — the scene stays dark");
                m_hist[0].reset();
                m_hist[1].reset();
                return;
            }
        }
        // SRB sets: plasma binds ubuf(0) + the READ texture(1); the blit
        // binds only the WRITE texture(1) (its shader declares nothing
        // else). Both bilinear (the reference's prev_samp is bilinear).
        for (int i = 0; i < 2; ++i) {
            m_srbPlasma[i].reset(r->newShaderResourceBindings());
            m_srbPlasma[i]->setBindings({
                QRhiShaderResourceBinding::uniformBuffer(
                    0,
                    QRhiShaderResourceBinding::VertexStage
                        | QRhiShaderResourceBinding::FragmentStage,
                    m_ubuf.get()),
                QRhiShaderResourceBinding::sampledTexture(
                    1, QRhiShaderResourceBinding::FragmentStage,
                    m_hist[i].get(), m_sampler.get()),
            });
            m_srbBlit[i].reset(r->newShaderResourceBindings());
            m_srbBlit[i]->setBindings({
                QRhiShaderResourceBinding::sampledTexture(
                    1, QRhiShaderResourceBinding::FragmentStage,
                    m_hist[i].get(), m_sampler.get()),
            });
            if (!m_srbPlasma[i]->create() || !m_srbBlit[i]->create()) {
                qWarning("[plasma] shader resource bindings failed — the scene stays dark");
                m_hist[0].reset();
                m_hist[1].reset();
                return;
            }
        }
    }

    void setVertexLayout(QRhiGraphicsPipeline *pipe)
    {
        QRhiVertexInputLayout inputLayout;
        inputLayout.setBindings({ QRhiVertexInputBinding(quint32(2 * sizeof(float))) });
        inputLayout.setAttributes({
            { 0, 0, QRhiVertexInputAttribute::Float2, 0 }, // pos
        });
        pipe->setVertexInputLayout(inputLayout);
    }

    void buildPlasmaPipeline()
    {
        if (!m_plasmaPipe)
            m_plasmaPipe.reset(rhi()->newGraphicsPipeline());
        m_plasmaPipe->setTopology(QRhiGraphicsPipeline::Triangles);
        m_plasmaPipe->setCullMode(QRhiGraphicsPipeline::None);
        m_plasmaPipe->setDepthTest(false);
        m_plasmaPipe->setDepthWrite(false);
        // OPAQUE output (the FS writes alpha 1; the scene owns the
        // background while active — the linebed header documents the same
        // choice for its explicit blend).
        m_plasmaPipe->setShaderStages({
            { QRhiShaderStage::Vertex,
              loadShader(QStringLiteral(
                  ":/qt/qml/app/chilpa/graphics/assets/shaders/rhi_triangle.vert.qsb")) },
            { QRhiShaderStage::Fragment,
              loadShader(QStringLiteral(
                  ":/qt/qml/app/chilpa/graphics/assets/shaders/plasma.frag.qsb")) },
        });
        setVertexLayout(m_plasmaPipe.get());
        m_plasmaPipe->setShaderResourceBindings(m_srbPlasma[0].get());
        m_plasmaPipe->setRenderPassDescriptor(m_plasmaPassDescriptor);
        if (!m_plasmaPipe->create()) {
            qWarning("[plasma] pipeline creation failed — the scene stays dark");
            m_plasmaPipe.reset();
        }
    }

    void buildBlitPipeline()
    {
        if (!m_blitPipe)
            m_blitPipe.reset(rhi()->newGraphicsPipeline());
        m_blitPipe->setTopology(QRhiGraphicsPipeline::Triangles);
        m_blitPipe->setSampleCount(renderTarget()->sampleCount());
        m_blitPipe->setCullMode(QRhiGraphicsPipeline::None);
        m_blitPipe->setDepthTest(false);
        m_blitPipe->setDepthWrite(false);
        m_blitPipe->setShaderStages({
            { QRhiShaderStage::Vertex,
              loadShader(QStringLiteral(
                  ":/qt/qml/app/chilpa/graphics/assets/shaders/rhi_triangle.vert.qsb")) },
            { QRhiShaderStage::Fragment,
              loadShader(QStringLiteral(
                  ":/qt/qml/app/chilpa/graphics/assets/shaders/plasma_blit.frag.qsb")) },
        });
        setVertexLayout(m_blitPipe.get());
        m_blitPipe->setShaderResourceBindings(m_srbBlit[0].get());
        m_blitPipe->setRenderPassDescriptor(m_blitPassDescriptor);
        if (!m_blitPipe->create()) {
            qWarning("[plasma] blit pipeline creation failed — the scene stays dark");
            m_blitPipe.reset();
        }
    }

    std::unique_ptr<QRhiBuffer> m_vbuf;
    std::unique_ptr<QRhiBuffer> m_ubuf;
    std::unique_ptr<QRhiSampler> m_sampler;
    std::unique_ptr<QRhiTexture> m_hist[2];
    std::unique_ptr<QRhiTextureRenderTarget> m_histRt[2];
    // Render pass descriptors owned BY US (the RT borrows them) — see the
    // rebuild loop. Raw pointers, freed on rebuild.
    QRhiRenderPassDescriptor *m_histRpDesc[2] = { nullptr, nullptr };
    std::unique_ptr<QRhiShaderResourceBindings> m_srbPlasma[2];
    std::unique_ptr<QRhiShaderResourceBindings> m_srbBlit[2];
    std::unique_ptr<QRhiGraphicsPipeline> m_plasmaPipe;
    std::unique_ptr<QRhiGraphicsPipeline> m_blitPipe;
    QRhiRenderPassDescriptor *m_plasmaPassDescriptor = nullptr;
    QRhiRenderPassDescriptor *m_blitPassDescriptor = nullptr;
    QSize m_histSize;
    int m_read = 0;
    bool m_needClear = true;

    // synchronize() snapshots.
    float m_time = 0.0f;
    float m_beat = 0.0f;
    float m_level = 0.0f;
    float m_levelSmooth = 0.0f;
    QVector4D m_energyLo{ 0.0f, 0.0f, 0.0f, 0.0f };
    QVector4D m_energyHi{ 0.0f, 0.0f, 0.0f, 0.0f };
    float m_transientAmp = 0.0f;
    QVector4D m_bandsLo{ 0.0f, 0.0f, 0.0f, 0.0f };
    QVector4D m_bandsHi{ 0.0f, 0.0f, 0.0f, 0.0f };
    QVector4D m_primary{ 0.0f, 0.863f, 0.784f, 1.0f };
    QVector4D m_secondary{ 0.588f, 0.196f, 1.0f, 1.0f };
    QVector4D m_accent{ 0.247f, 0.851f, 0.784f, 1.0f };
};

} // namespace

QQuickRhiItemRenderer *PlasmaItem::createRenderer()
{
    return new PlasmaRenderer;
}

// QML type registration — the linebed_item.cpp idiom: the module is STATIC,
// so a startup function runs at QGuiApplication construction, BEFORE the
// engine loads the QML that names PlasmaItem.
static bool g_plasmaRegistered = false;

extern "C" void chilpa_plasma_register_qml_type()
{
    if (g_plasmaRegistered)
        return;
    g_plasmaRegistered = true;
    qmlRegisterType<PlasmaItem>("app.chilpa.graphics", 1, 0, "PlasmaItem");
}

Q_COREAPP_STARTUP_FUNCTION(chilpa_plasma_register_qml_type)
