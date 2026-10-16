// Copyright (c) 2024 blitzkriegfc. MIT; see licenses/immersive-renderers-MIT.txt.
// Line Bed (immersive shader scene, mode 5) — the QRhi renderer behind
// LineBedItem. Ports the Slint line-strip pipeline
// (crates/qbz/src/shader_underlay.rs:689-727) and its draw call
// (:995-998): 200 INSTANCED line strips of (255*6 + 1) = 1531 vertices
// each, alpha blending, the heights ring as an R32F texture sampled in the
// VERTEX stage. The vertex/instance ids the WGSL took from
// @builtin(vertex_index)/@builtin(instance_index) arrive here as two float
// vertex attributes (1531-vert template buffer + a 200-entry per-instance
// buffer) — the maximally portable spelling across GL/GLES/Vulkan/Metal/
// D3D under QRhi.
//
// Resource cadence (the pulse law): `synchronize` snapshots the QML-side
// properties when the item updates (pulse tick only), `render` uploads the
// heights texture only when a NEW ring arrived, and the draw itself is
// ~306K line vertices at 30 Hz — trivial next to a fullscreen fragment
// scene.

#include "linebed_item.h"

#include <QtCore/QCoreApplication>
#include <QtCore/QFile>
#include <QtGui/QVector4D>
#include <QtQml/qqml.h>
#include <rhi/qrhi.h>

#include <cstring>
#include <vector>

namespace {

// The lattice — pinned by FOUR files that must agree: here, linebed.vert
// (NUM_BANDS/SUBDIV), src/linebed_qt.rs (LINEBED_LINES/LINEBED_BANDS) and
// the Slint reference (shader_underlay.rs:47-54).
constexpr int LINEBED_LINES = 200;
constexpr int LINEBED_BANDS = 256;
constexpr int LINEBED_SUBDIV = 6;
constexpr int VERTS_PER_LINE = (LINEBED_BANDS - 1) * LINEBED_SUBDIV + 1; // 1531
constexpr qsizetype HEIGHTS_BYTES = LINEBED_LINES * LINEBED_BANDS * qsizetype(sizeof(float));

// std140: vec2 + vec4 + vec4. The GLSL block (linebed.vert/.frag, binding 0):
//     layout(std140, binding = 0) uniform buf {
//         vec2 resolution;  //   0
//         vec4 primary;     //  16
//         vec4 accent;      //  32
//     };
// The WGSL reference reads the full 144-byte scene block but uses only
// resolution (VS) and primary/accent (FS) — line_bed.wgsl is the one scene
// with no time/level/energy input, so the block is trimmed to what the
// shader reads. Same field VALUES at the same meaning.
struct UniformData
{
    float res[2];
    float pad[2];
    float primary[4];
    float accent[4];
};

QShader loadShader(const QString &qrcPath)
{
    QFile f(qrcPath);
    if (f.open(QIODevice::ReadOnly)) {
        const QShader s = QShader::fromSerialized(f.readAll());
        if (!s.isValid())
            qWarning("[linebed] shader %s failed to deserialize", qPrintable(qrcPath));
        return s;
    }
    qWarning("[linebed] shader %s not found in the qrc", qPrintable(qrcPath));
    return QShader();
}

class LineBedRenderer : public QQuickRhiItemRenderer
{
public:
    void initialize(QRhiCommandBuffer *cb) override
    {
        QRhi *r = cb->rhi();
        if (m_resourceOwner && m_resourceOwner != r) {
            m_pipeline.reset();
            m_srb.reset();
            m_sampler.reset();
            m_tex.reset();
            m_ubuf.reset();
            m_ibuf.reset();
            m_vbuf.reset();
            m_passDescriptor = nullptr;
            m_heightsDirty = !m_heights.isEmpty();
        }
        m_resourceOwner = r;
        if (m_vbuf)
            return;
        m_vbuf.reset(r->newBuffer(QRhiBuffer::Immutable,
                                  QRhiBuffer::VertexBuffer,
                                  VERTS_PER_LINE * quint32(sizeof(float))));
        m_ibuf.reset(r->newBuffer(QRhiBuffer::Immutable,
                                  QRhiBuffer::VertexBuffer,
                                  LINEBED_LINES * quint32(sizeof(float))));
        m_ubuf.reset(r->newBuffer(QRhiBuffer::Dynamic,
                                  QRhiBuffer::UniformBuffer,
                                  quint32(sizeof(UniformData))));
        m_tex.reset(r->newTexture(QRhiTexture::R32F, QSize(LINEBED_BANDS, LINEBED_LINES)));
        m_sampler.reset(r->newSampler(QRhiSampler::Nearest,
                                      QRhiSampler::Nearest,
                                      QRhiSampler::None,
                                      QRhiSampler::ClampToEdge,
                                      QRhiSampler::ClampToEdge));
        if (!m_vbuf->create() || !m_ibuf->create() || !m_ubuf->create()
            || !m_tex->create() || !m_sampler->create()) {
            qWarning("[linebed] resource creation failed — the scene stays dark");
            m_vbuf.reset();
            return;
        }
        m_srb.reset(r->newShaderResourceBindings());
        m_srb->setBindings({
            QRhiShaderResourceBinding::uniformBuffer(
                0,
                QRhiShaderResourceBinding::VertexStage
                    | QRhiShaderResourceBinding::FragmentStage,
                m_ubuf.get()),
            // Binding 4, like the reference's @binding(4) heights_tex.
            QRhiShaderResourceBinding::sampledTexture(
                4, QRhiShaderResourceBinding::VertexStage, m_tex.get(), m_sampler.get()),
        });
        if (!m_srb->create()) {
            qWarning("[linebed] shader resource bindings failed — the scene stays dark");
            m_vbuf.reset();
            return;
        }

        QRhiResourceUpdateBatch *u = r->nextResourceUpdateBatch();
        // The vertex templates: vid = subdivided point index along the line
        // (0..1530), line = the depth row per instance (0..199).
        std::vector<float> vid(VERTS_PER_LINE);
        for (int i = 0; i < VERTS_PER_LINE; ++i)
            vid[i] = float(i);
        u->uploadStaticBuffer(m_vbuf.get(), 0,
                              quint32(vid.size() * sizeof(float)), vid.data());
        std::vector<float> lines(LINEBED_LINES);
        for (int i = 0; i < LINEBED_LINES; ++i)
            lines[i] = float(i);
        u->uploadStaticBuffer(m_ibuf.get(), 0,
                              quint32(lines.size() * sizeof(float)), lines.data());
        // Flat bed until the first ring publish (the Rust ring starts
        // zeroed too — linebed_qt.rs LineBedState::default).
        const QByteArray zeros(int(HEIGHTS_BYTES), 0);
        QRhiTextureSubresourceUploadDescription sub(zeros);
        sub.setDataStride(LINEBED_BANDS * quint32(sizeof(float)));
        u->uploadTexture(m_tex.get(),
                         QRhiTextureUploadDescription(QRhiTextureUploadEntry(0, 0, sub)));
        cb->resourceUpdate(u);
    }

    void synchronize(QQuickRhiItem *item) override
    {
        // GUI thread: snapshot the item's properties. Runs on item update()
        // (the pulse tick) and on geometry/rhi changes.
        auto *lb = static_cast<LineBedItem *>(item);
        const QByteArray h = lb->heights();
        if (h.size() == HEIGHTS_BYTES && h != m_heights) {
            m_heights = h;
            m_heightsDirty = true;
        }
        const QColor p = lb->primary();
        const QColor a = lb->accent();
        m_primary = QVector4D(float(p.redF()), float(p.greenF()), float(p.blueF()), 1.0f);
        m_accent = QVector4D(float(a.redF()), float(a.greenF()), float(a.blueF()), 1.0f);
    }

    void render(QRhiCommandBuffer *cb) override
    {
        if (!m_vbuf)
            return; // resource creation failed in initialize
        QRhiRenderTarget *rt = renderTarget();
        if (!m_pipeline || rt->renderPassDescriptor() != m_passDescriptor) {
            m_passDescriptor = rt->renderPassDescriptor();
            buildPipeline();
            if (!m_pipeline)
                return;
        }

        QRhiResourceUpdateBatch *u = rhi()->nextResourceUpdateBatch();
        if (m_heightsDirty) {
            m_heightsDirty = false;
            QRhiTextureSubresourceUploadDescription sub(m_heights);
            sub.setDataStride(LINEBED_BANDS * quint32(sizeof(float)));
            u->uploadTexture(m_tex.get(),
                             QRhiTextureUploadDescription(QRhiTextureUploadEntry(0, 0, sub)));
        }

        const QSize ps = rt->pixelSize();
        UniformData ud = {};
        ud.res[0] = float(ps.width());
        ud.res[1] = float(ps.height());
        std::memcpy(ud.primary, &m_primary, sizeof(ud.primary));
        std::memcpy(ud.accent, &m_accent, sizeof(ud.accent));
        u->updateDynamicBuffer(m_ubuf.get(), 0, quint32(sizeof(ud)), &ud);

        // Opaque black clear, like the reference's LoadOp::Clear(BLACK):
        // the scene OWNS the background while active (the atmosphere and
        // panels gate themselves off), and the item composites opaquely
        // (alphaBlending stays false). The FS alpha rides the explicit
        // SrcAlpha blend of the pipeline, exactly wgpu ALPHA_BLENDING.
        cb->beginPass(rt, QColor(0, 0, 0, 255), QRhiDepthStencilClearValue(1.0f, 0), u);
        cb->setGraphicsPipeline(m_pipeline.get());
        cb->setViewport(QRhiViewport(0, 0, float(ps.width()), float(ps.height())));
        const QRhiCommandBuffer::VertexInput vbufs[] = {
            QRhiCommandBuffer::VertexInput(m_vbuf.get(), 0),
            QRhiCommandBuffer::VertexInput(m_ibuf.get(), 0),
        };
        cb->setShaderResources(m_srb.get());
        cb->setVertexInput(0, 2, vbufs);
        cb->draw(VERTS_PER_LINE, LINEBED_LINES); // 200 instanced strips of 1531
        cb->endPass();
    }

private:
    QRhi *m_resourceOwner = nullptr;
    void buildPipeline()
    {
        if (!m_pipeline)
            m_pipeline.reset(rhi()->newGraphicsPipeline());
        m_pipeline->setTopology(QRhiGraphicsPipeline::LineStrip);
        m_pipeline->setSampleCount(renderTarget()->sampleCount());
        m_pipeline->setCullMode(QRhiGraphicsPipeline::None);
        m_pipeline->setDepthTest(false);
        m_pipeline->setDepthWrite(false);
        // wgpu BlendState::ALPHA_BLENDING verbatim.
        QRhiGraphicsPipeline::TargetBlend blend;
        blend.enable = true;
        blend.srcColor = QRhiGraphicsPipeline::SrcAlpha;
        blend.dstColor = QRhiGraphicsPipeline::OneMinusSrcAlpha;
        blend.srcAlpha = QRhiGraphicsPipeline::One;
        blend.dstAlpha = QRhiGraphicsPipeline::OneMinusSrcAlpha;
        m_pipeline->setTargetBlends({ blend });
        m_pipeline->setShaderStages({
            { QRhiShaderStage::Vertex,
              loadShader(QStringLiteral(
                  ":/qt/qml/app/chilpa/graphics/assets/shaders/linebed.vert.qsb")) },
            { QRhiShaderStage::Fragment,
              loadShader(QStringLiteral(
                  ":/qt/qml/app/chilpa/graphics/assets/shaders/linebed.frag.qsb")) },
        });
        QRhiVertexInputLayout inputLayout;
        inputLayout.setBindings({
            QRhiVertexInputBinding(quint32(sizeof(float))),
            QRhiVertexInputBinding(quint32(sizeof(float)),
                                   QRhiVertexInputBinding::PerInstance),
        });
        inputLayout.setAttributes({
            { 0, 0, QRhiVertexInputAttribute::Float, 0 }, // vid
            { 1, 1, QRhiVertexInputAttribute::Float, 0 }, // line (per instance)
        });
        m_pipeline->setVertexInputLayout(inputLayout);
        m_pipeline->setShaderResourceBindings(m_srb.get());
        m_pipeline->setRenderPassDescriptor(m_passDescriptor);
        if (!m_pipeline->create()) {
            qWarning("[linebed] pipeline creation failed — the scene stays dark");
            m_pipeline.reset();
        }
    }

    std::unique_ptr<QRhiBuffer> m_vbuf;
    std::unique_ptr<QRhiBuffer> m_ibuf;
    std::unique_ptr<QRhiBuffer> m_ubuf;
    std::unique_ptr<QRhiTexture> m_tex;
    std::unique_ptr<QRhiSampler> m_sampler;
    std::unique_ptr<QRhiShaderResourceBindings> m_srb;
    std::unique_ptr<QRhiGraphicsPipeline> m_pipeline;
    QRhiRenderPassDescriptor *m_passDescriptor = nullptr;

    QByteArray m_heights;
    bool m_heightsDirty = false;
    // The Slint default palette (see the header) until synchronize() runs.
    QVector4D m_primary{ 0.0f, 0.863f, 0.784f, 1.0f };
    QVector4D m_accent{ 0.247f, 0.851f, 0.784f, 1.0f };
};

} // namespace

QQuickRhiItemRenderer *LineBedItem::createRenderer()
{
    return new LineBedRenderer;
}

// QML type registration. The QML module is STATIC (linked into the
// binary), so there is no plugin load to hang this on: the startup function
// runs at QGuiApplication construction, BEFORE the engine loads the QML
// that names LineBedItem (ShaderSceneLayer.qml is compiled during the
// initial load; Component.onCompleted — where the singleton boots run — is
// already too late).
static bool g_linebedRegistered = false;

extern "C" void chilpa_line_bed_register_qml_type()
{
    if (g_linebedRegistered)
        return;
    g_linebedRegistered = true;
    qmlRegisterType<LineBedItem>("app.chilpa.graphics", 1, 0, "LineBedItem");
}

Q_COREAPP_STARTUP_FUNCTION(chilpa_line_bed_register_qml_type)
