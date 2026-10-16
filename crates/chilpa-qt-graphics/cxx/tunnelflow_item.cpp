// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
// Tunnel Flow (immersive shader scene, mode 8 — Qt-only) — the QRhi
// renderer behind TunnelFlowItem. Block B1 of the 2026-08-15
// immersive-completion contract (spec 02-tauri-tunnel-port.md). The
// mechanism is the PlasmaItem one (cxx/plasma_item.cpp carries the full
// rationale): a PING-PONG over two owned RGBA8 textures — pass 1 renders the
// tunnel into history[write] sampling history[read] (the source panel's
// self-drawImage trail system), pass 2 blits history[write] into the item's
// render target, then the indices swap. Owning both targets means the item's
// texture is never read back and no Qt-internal attachment is touched. The
// blit reuses plasma_blit.frag.qsb (a pure 1:1 textured triangle).
//
// Resource cadence (the pulse law): `synchronize` snapshots the QML-side
// properties when the item updates (pulse tick only); per frame the GPU work
// is one uniform upload + two fullscreen triangles.
//
// History size tracks the item's physical size CAPPED at 2560x1440 (the
// plasma_item / ShaderSceneLayer resW/resH rule); on a bigger screen the
// blit upscales. A resize rebuilds the pair and re-clears it (the trails
// restart).

#include "tunnelflow_item.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QFile>
#include <QtQml/qqml.h>
#include <rhi/qrhi.h>

#include <cstring>

namespace {

// The history-target ceiling (see the header).
constexpr int MAX_W = 2560;
constexpr int MAX_H = 1440;

// The scene's own std140 block — NOT the 144-byte spec-01 block: Tunnel Flow
// consumes the Viz16 stream (smoothed in QML), not the shader pack, so the
// block holds exactly what tunnel_flow.frag reads. vec4-aligned.
struct UniformData
{
    float time;        //   0 (seconds)
    float phase;       //   4
    float bass;        //   8
    float mid;         //  12
    float high;        //  16
    float kick;        //  20
    float res[2];      //  24
    float palette0[4]; //  32
    float palette1[4]; //  48
    float palette2[4]; //  64
    float palette3[4]; //  80
};
static_assert(sizeof(UniformData) == 96, "std140 tunnel-flow block");

QShader loadShader(const QString &qrcPath)
{
    QFile f(qrcPath);
    if (f.open(QIODevice::ReadOnly)) {
        const QShader s = QShader::fromSerialized(f.readAll());
        if (!s.isValid())
            qWarning("[tunnelflow] shader %s failed to deserialize", qPrintable(qrcPath));
        return s;
    }
    qWarning("[tunnelflow] shader %s not found in the qrc", qPrintable(qrcPath));
    return QShader();
}

QVector4D colorVec(const QColor &c)
{
    return QVector4D(float(c.redF()), float(c.greenF()), float(c.blueF()), 1.0f);
}

class TunnelFlowRenderer : public QQuickRhiItemRenderer
{
public:
    ~TunnelFlowRenderer() override
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
            m_scenePipe.reset();
            for (int i = 0; i < 2; ++i) {
                m_srbBlit[i].reset();
                m_srbScene[i].reset();
                m_histRt[i].reset();
                delete m_histRpDesc[i];
                m_histRpDesc[i] = nullptr;
                m_hist[i].reset();
            }
            m_sampler.reset();
            m_ubuf.reset();
            m_vbuf.reset();
            m_scenePassDescriptor = nullptr;
            m_blitPassDescriptor = nullptr;
            m_histSize = QSize();
        }
        m_resourceOwner = r;
        if (m_vbuf)
            return;
        // The fullscreen triangle (rhi_triangle.vert replaces gl_VertexID
        // with this buffer — the GLES 3.0-portable spelling, plasma idiom).
        // Shared by the feedback AND blit passes.
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
            qWarning("[tunnelflow] resource creation failed — the scene stays dark");
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
        auto *tf = static_cast<TunnelFlowItem *>(item);
        m_time = tf->m_time;
        m_phase = tf->m_phase;
        m_bass = tf->m_bass;
        m_mid = tf->m_mid;
        m_high = tf->m_high;
        m_kick = tf->m_kick;
        m_palette0 = colorVec(tf->m_palette0);
        m_palette1 = colorVec(tf->m_palette1);
        m_palette2 = colorVec(tf->m_palette2);
        m_palette3 = colorVec(tf->m_palette3);
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

        // Pipeline A (scene) renders into the history pair's pass; pipeline
        // B (blit) into the item's. Rebuild each when its descriptor moves.
        if (!m_scenePipe || m_histRt[0]->renderPassDescriptor() != m_scenePassDescriptor) {
            m_scenePassDescriptor = m_histRt[0]->renderPassDescriptor();
            buildScenePipeline();
            if (!m_scenePipe)
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
        ud.phase = m_phase;
        ud.bass = m_bass;
        ud.mid = m_mid;
        ud.high = m_high;
        ud.kick = m_kick;
        ud.res[0] = float(hist.width());
        ud.res[1] = float(hist.height());
        std::memcpy(ud.palette0, &m_palette0, sizeof(ud.palette0));
        std::memcpy(ud.palette1, &m_palette1, sizeof(ud.palette1));
        std::memcpy(ud.palette2, &m_palette2, sizeof(ud.palette2));
        std::memcpy(ud.palette3, &m_palette3, sizeof(ud.palette3));
        u->updateDynamicBuffer(m_ubuf.get(), 0, quint32(sizeof(ud)), &ud);

        // A fresh pair starts BLACK (the source panel's canvas starts blank
        // too — an unsampled-cleared first frame would advect garbage).
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

        // Pass 1: the tunnel into history[write], sampling history[read].
        cb->beginPass(m_histRt[write].get(), QColor(0, 0, 0, 255),
                      QRhiDepthStencilClearValue(1.0f, 0), u);
        cb->setGraphicsPipeline(m_scenePipe.get());
        cb->setViewport(QRhiViewport(0, 0, float(hist.width()), float(hist.height())));
        cb->setShaderResources(m_srbScene[read].get());
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
            if (m_srbScene[i])
                m_srbScene[i].release()->deleteLater();
            if (m_srbBlit[i])
                m_srbBlit[i].release()->deleteLater();
        }
        if (m_scenePipe)
            m_scenePipe.release()->deleteLater();
        m_scenePassDescriptor = nullptr;
        m_histSize = size;
        m_needClear = true;
        m_read = 0;

        for (int i = 0; i < 2; ++i) {
            // The feedback texture is sampled on one pass and is the color
            // target on the next. D3D requires the RenderTarget usage bit;
            // without it Tunnel Flow stays black on Windows.
            m_hist[i].reset(r->newTexture(QRhiTexture::RGBA8,
                                          size,
                                          1,
                                          QRhiTexture::RenderTarget));
            if (!m_hist[i]->create()) {
                qWarning("[tunnelflow] history texture creation failed — the scene stays dark");
                m_hist[0].reset();
                m_hist[1].reset();
                return;
            }
            // Vulkan/D3D/Metal REQUIRE a render pass descriptor before
            // create(); without it QVkTextureRenderTarget warns and the
            // later beginPass segfaults (2026-08-15 owner smoke: Tunnel
            // Flow killed the app). The RT does NOT own the descriptor —
            // free the previous one on rebuild.
            if (m_histRpDesc[i]) {
                m_histRpDesc[i]->deleteLater();
                m_histRpDesc[i] = nullptr;
            }
            m_histRt[i].reset(r->newTextureRenderTarget({ m_hist[i].get() }));
            m_histRpDesc[i] = m_histRt[i]->newCompatibleRenderPassDescriptor();
            m_histRt[i]->setRenderPassDescriptor(m_histRpDesc[i]);
            if (!m_histRt[i]->create()) {
                qWarning("[tunnelflow] history render target failed — the scene stays dark");
                m_hist[0].reset();
                m_hist[1].reset();
                return;
            }
        }
        // SRB sets: the scene pass binds ubuf(0) + the READ texture(1); the
        // blit binds only the WRITE texture(1) (its shader declares nothing
        // else). Both bilinear.
        for (int i = 0; i < 2; ++i) {
            m_srbScene[i].reset(r->newShaderResourceBindings());
            m_srbScene[i]->setBindings({
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
            if (!m_srbScene[i]->create() || !m_srbBlit[i]->create()) {
                qWarning("[tunnelflow] shader resource bindings failed — the scene stays dark");
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

    void buildScenePipeline()
    {
        if (!m_scenePipe)
            m_scenePipe.reset(rhi()->newGraphicsPipeline());
        m_scenePipe->setTopology(QRhiGraphicsPipeline::Triangles);
        m_scenePipe->setCullMode(QRhiGraphicsPipeline::None);
        m_scenePipe->setDepthTest(false);
        m_scenePipe->setDepthWrite(false);
        // OPAQUE output (the FS writes alpha 1; the scene owns the
        // background while active — the plasma header documents the same
        // choice).
        m_scenePipe->setShaderStages({
            { QRhiShaderStage::Vertex,
              loadShader(QStringLiteral(
                  ":/qt/qml/app/chilpa/graphics/assets/shaders/rhi_triangle.vert.qsb")) },
            { QRhiShaderStage::Fragment,
              loadShader(QStringLiteral(
                  ":/qt/qml/app/chilpa/graphics/assets/shaders/tunnel_flow.frag.qsb")) },
        });
        setVertexLayout(m_scenePipe.get());
        m_scenePipe->setShaderResourceBindings(m_srbScene[0].get());
        m_scenePipe->setRenderPassDescriptor(m_scenePassDescriptor);
        if (!m_scenePipe->create()) {
            qWarning("[tunnelflow] pipeline creation failed — the scene stays dark");
            m_scenePipe.reset();
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
            qWarning("[tunnelflow] blit pipeline creation failed — the scene stays dark");
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
    std::unique_ptr<QRhiShaderResourceBindings> m_srbScene[2];
    std::unique_ptr<QRhiShaderResourceBindings> m_srbBlit[2];
    std::unique_ptr<QRhiGraphicsPipeline> m_scenePipe;
    std::unique_ptr<QRhiGraphicsPipeline> m_blitPipe;
    QRhiRenderPassDescriptor *m_scenePassDescriptor = nullptr;
    QRhiRenderPassDescriptor *m_blitPassDescriptor = nullptr;
    QSize m_histSize;
    int m_read = 0;
    bool m_needClear = true;

    // synchronize() snapshots.
    float m_time = 0.0f;
    float m_phase = 0.0f;
    float m_bass = 0.0f;
    float m_mid = 0.0f;
    float m_high = 0.0f;
    float m_kick = 0.0f;
    QVector4D m_palette0{ 1.0f, 0.416f, 0.416f, 1.0f };
    QVector4D m_palette1{ 1.0f, 0.804f, 0.361f, 1.0f };
    QVector4D m_palette2{ 0.408f, 0.863f, 0.667f, 1.0f };
    QVector4D m_palette3{ 0.431f, 0.690f, 1.0f, 1.0f };
};

} // namespace

QQuickRhiItemRenderer *TunnelFlowItem::createRenderer()
{
    return new TunnelFlowRenderer;
}

// QML type registration — the plasma_item.cpp idiom: the module is STATIC,
// so a startup function runs at QGuiApplication construction, BEFORE the
// engine loads the QML that names TunnelFlowItem.
static bool g_tunnelflowRegistered = false;

extern "C" void chilpa_tunnel_flow_register_qml_type()
{
    if (g_tunnelflowRegistered)
        return;
    g_tunnelflowRegistered = true;
    qmlRegisterType<TunnelFlowItem>("app.chilpa.graphics", 1, 0, "TunnelFlowItem");
}

Q_COREAPP_STARTUP_FUNCTION(chilpa_tunnel_flow_register_qml_type)
