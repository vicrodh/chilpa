/* SPDX-License-Identifier: MIT */
#include "dsd.h"
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static void stream_state(void *data, enum pw_stream_state old,
        enum pw_stream_state state, const char *error)
{
    (void)old;
    struct dsd_session *s = data;
    s->state = state;
    if (state == PW_STREAM_STATE_ERROR)
        dsd_fail(s, -EIO, error ? error : "PipeWire DSD stream failed");
    else if (state == PW_STREAM_STATE_UNCONNECTED && s->opened)
        dsd_fail(s, -ENODEV, "PipeWire DSD stream disconnected");
    pw_thread_loop_signal(s->loop, false);
}

void dsd_stream_format(void *data, uint32_t id, const struct spa_pod *param)
{
    struct dsd_session *s = data;
    if (id != SPA_PARAM_Format || s->closing) return;
    s->format_valid = dsd_format_matches(param, s->rate, s->interleave);
    if (!s->format_valid) {
        /* A cleared format is a suspended stream: primed before its first
         * write, paused or flushed. Activation negotiates it again and waits
         * for the route. Another format, or losing it while live, fails. */
        if (param || s->live) {
            struct spa_audio_info_dsd info = { 0 };
            uint32_t type = 0, subtype = 0;
            if (param) {
                spa_format_parse(param, &type, &subtype);
                spa_format_audio_dsd_parse(param, &info);
            }
            char detail[DSD_ERROR_BYTES];
            snprintf(detail, sizeof(detail),
                "Unexpected DSD format (type=%u/%u rate=%u order=%u interleave=%d channels=%u positions=%u,%u)",
                type, subtype, info.rate, info.bitorder, info.interleave,
                info.channels, info.position[0], info.position[1]);
            dsd_fail(s, -EPROTO, detail);
        }
        return;
    }
    uint8_t storage[512];
    struct spa_pod_builder builder = SPA_POD_BUILDER_INIT(storage, sizeof(storage));
    const struct spa_pod *buffers = spa_pod_builder_add_object(&builder,
        SPA_TYPE_OBJECT_ParamBuffers, SPA_PARAM_Buffers,
        SPA_PARAM_BUFFERS_buffers, SPA_POD_CHOICE_RANGE_Int(4, 2, DSD_MAX_BUFFERS),
        SPA_PARAM_BUFFERS_blocks, SPA_POD_Int(1),
        SPA_PARAM_BUFFERS_size, SPA_POD_CHOICE_RANGE_Int(32768, 64, DSD_MAX_BUFFER_BYTES),
        SPA_PARAM_BUFFERS_stride, SPA_POD_Int(DSD_FRAME_BYTES));
    int result = pw_stream_update_params(s->stream, &buffers, 1);
    if (result < 0) dsd_fail(s, result, "Cannot negotiate bounded DSD buffers");
    pw_thread_loop_signal(s->loop, false);
}

static void add_buffer(void *data, struct pw_buffer *buffer)
{
    struct dsd_session *s = data;
    if (++s->buffer_count > DSD_MAX_BUFFERS || buffer->buffer->n_datas != 1 ||
            buffer->buffer->datas[0].maxsize > DSD_MAX_BUFFER_BYTES)
        dsd_fail(s, -E2BIG, "PipeWire DSD buffer budget exceeded");
    pw_thread_loop_signal(s->loop, false);
}

static void remove_buffer(void *data, struct pw_buffer *buffer)
{
    (void)buffer;
    struct dsd_session *s = data;
    if (s->buffer_count) s->buffer_count--;
}

static uint64_t now_ns(void)
{
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (uint64_t)now.tv_sec * 1000000000ull + (uint64_t)now.tv_nsec;
}

/* A drained stream is no longer playing: PipeWire may suspend it, and that
 * clears its format without being a failure. */
static void stream_drained(void *data)
{
    struct dsd_session *s = data;
    s->drained = true;
    s->active = false;
    s->live = false;
    s->last_delay = 0;
    pw_thread_loop_signal(s->loop, false);
}

/* PipeWire reports a drain once its queue is empty, not once the device has
 * played the last chunk: the drain reports done after the device's delay,
 * measured the first time the drain is seen. */
/* The device delay, when PipeWire can tell it: only while streaming. */
static void device_delay(struct dsd_session *s, int64_t *delay, uint32_t *num, uint32_t *denom)
{
    struct pw_time time = { 0 };
    if (pw_stream_get_time_n(s->stream, &time, sizeof(time)) >= 0 && time.rate.denom) {
        *delay = time.delay;
        *num = time.rate.num;
        *denom = time.rate.denom;
    }
}

static bool settled(struct dsd_session *s)
{
    if (!s->drained) return false;
    if (!s->settle_until) {
        /* What the drain read while streaming, unless PipeWire still says. */
        int64_t delay = s->drain_delay;
        uint32_t num = s->drain_num, denom = s->drain_denom;
        device_delay(s, &delay, &num, &denom);
        s->settle_until = now_ns() + dsd_settle_ns(s->rate, delay, num, denom, s->last_chunk);
    }
    return now_ns() >= s->settle_until;
}

static void process(void *data)
{
    struct dsd_session *s = data;
    if (s->drain_requested) return;
    /* An empty chunk is never queued: this stream is asynchronous, and a
     * queued chunk takes the cycle's handoff, replacing payload the peer has
     * not read yet. Before the route of this activation is confirmed, and once
     * a drain has no payload left, nothing goes out and the sink plays its own
     * silence. */
    if (!s->opened || !s->live || !dsd_route_ready(s)) return;
    if (s->draining && s->write_at == s->read_at) return;
    struct pw_buffer *buffer = pw_stream_dequeue_buffer(s->stream);
    if (!buffer) return;
    struct spa_buffer *spa = buffer->buffer;
    if (spa->n_datas != 1 || !spa->datas[0].data || !spa->datas[0].chunk ||
            spa->datas[0].maxsize > DSD_MAX_BUFFER_BYTES) {
        dsd_fail(s, -EPROTO, "Invalid PipeWire DSD buffer");
        /* A fatal stream is destroyed by its owner; no invalid data is queued. */
        return;
    }
    struct spa_data *d = &spa->datas[0];
    size_t frames = d->maxsize / DSD_FRAME_BYTES;
    if (buffer->requested) frames = SPA_MIN(frames, buffer->requested);
    /* Negotiation, identity and links were checked above, under this same
     * loop lock. */
    size_t copied = dsd_ring_read(s, d->data, frames);
    size_t filled = copied;
    if (copied < frames && !s->draining) {
        /* Balanced DSD idle bytes, never PCM zeros. Not counted as user data. */
        memset((uint8_t *)d->data + copied * DSD_FRAME_BYTES, 0x69,
                (frames - copied) * DSD_FRAME_BYTES);
        filled = frames;
        s->xrun = true;
    }
    d->chunk->offset = 0;
    d->chunk->flags = 0;
    d->chunk->stride = DSD_FRAME_BYTES;
    d->chunk->size = filled * DSD_FRAME_BYTES;
    buffer->size = copied;
    s->last_chunk = filled;
    int result = pw_stream_queue_buffer(s->stream, buffer);
    if (result < 0) dsd_fail(s, result, "Cannot queue PipeWire DSD buffer");
}

static const struct pw_stream_events stream_events = {
    PW_VERSION_STREAM_EVENTS, .state_changed = stream_state, .param_changed = dsd_stream_format,
    .add_buffer = add_buffer, .remove_buffer = remove_buffer, .process = process,
    .drained = stream_drained,
};

int dsd_session_start_stream(struct dsd_session *s, const struct timespec *deadline)
{
    char serial[32], node_rate[32];
    snprintf(serial, sizeof(serial), "%llu", (unsigned long long)s->caps.serial);
    snprintf(node_rate, sizeof(node_rate), "1/%u", s->rate);
    struct pw_properties *props = pw_properties_new(
        PW_KEY_MEDIA_TYPE, "Audio", PW_KEY_MEDIA_CATEGORY, "Playback",
        PW_KEY_MEDIA_ROLE, "Music", PW_KEY_APP_NAME, "Chilpa",
        PW_KEY_NODE_NAME, "chilpa.native-dsd", PW_KEY_TARGET_OBJECT, serial,
        PW_KEY_NODE_RATE, node_rate, PW_KEY_NODE_EXCLUSIVE, "true",
        /* Since PipeWire 1.6 a reliable output never replaces a handoff the
         * peer has not read; older servers ignore it. Spelled out, because
         * older headers do not name it. */
        "node.reliable", "true",
        PW_KEY_NODE_DONT_RECONNECT, "true", "node.dont-fallback", "true",
        "node.dont-move", "true", NULL);
    if (!props) return -ENOMEM;
    s->stream = pw_stream_new(s->core, "Chilpa native DSD", props);
    if (!s->stream) return -errno;
    pw_stream_add_listener(s->stream, &s->stream_listener, &stream_events, s);
    struct spa_audio_info_dsd format = {
        .bitorder = SPA_PARAM_BITORDER_msb, .interleave = s->interleave,
        .rate = s->rate * 4u, .channels = 2,
        .position = { SPA_AUDIO_CHANNEL_FL, SPA_AUDIO_CHANNEL_FR },
    };
    uint8_t storage[512];
    struct spa_pod_builder builder = SPA_POD_BUILDER_INIT(storage, sizeof(storage));
    const struct spa_pod *param = spa_format_audio_dsd_build(&builder, SPA_PARAM_EnumFormat, &format);
    /* Main-loop process callbacks serialize route changes and payload admission.
     * No dependency on the producer running often enough to service the graph. */
    int result = pw_stream_connect(s->stream, PW_DIRECTION_OUTPUT, PW_ID_ANY,
        PW_STREAM_FLAG_AUTOCONNECT | PW_STREAM_FLAG_INACTIVE | PW_STREAM_FLAG_MAP_BUFFERS |
        PW_STREAM_FLAG_NO_CONVERT | PW_STREAM_FLAG_EXCLUSIVE | PW_STREAM_FLAG_DONT_RECONNECT,
        &param, 1);
    if (result < 0) return result;
    /* Link allocation can wait for an active node. During this handshake the
     * payload gate stays closed and process queues only empty chunks. */
    result = pw_stream_set_active(s->stream, true);
    if (result < 0) return result;
    for (unsigned pass = 0; !s->error && pass < DSD_MAX_LINKS; pass++) {
        while (!s->error && (!dsd_route_ready(s) || s->buffer_count < 2)) {
            result = pw_thread_loop_timed_wait_full(s->loop, deadline);
            if (result < 0 && (!dsd_route_ready(s) || s->buffer_count < 2)) {
                char detail[DSD_ERROR_BYTES];
                unsigned links = 0, total = 0;
                for (unsigned i = 0; i < DSD_MAX_LINKS; i++) {
                    if (s->links[i].valid) links++;
                    if (s->links[i].proxy) total++;
                }
                snprintf(detail, sizeof(detail),
                    "DSD negotiation timed out (state=%d format=%d links=%u/%u buffers=%u physical=%d)",
                    s->state, s->format_valid, links, total, s->buffer_count, s->physical);
                dsd_fail(s, result, detail);
                break;
            }
        }
        /* A registry event can bind a link whose info arrives one roundtrip
         * later. Stabilize those observations before granting the stream. */
        if (!s->error) dsd_roundtrip(s, deadline);
        if (dsd_route_ready(s)) break;
    }
    if (!s->error && !dsd_route_ready(s)) dsd_fail(s, -EPROTO, "DSD route is not ready");
    if (!s->error) {
        result = pw_stream_set_active(s->stream, false);
        if (result < 0) dsd_fail(s, result, "Cannot prime native DSD stream");
    }
    s->opened = s->error == 0;
    return s->error;
}

struct dsd_session *chilpa_pw_dsd_open(const char *name, uint64_t serial,
        uint32_t cookie, uint32_t rate, int32_t interleave, char *error, size_t size)
{
    struct timespec deadline;
    struct dsd_session *s = dsd_session_new(name, serial, cookie, &deadline);
    if (!s) { snprintf(error, size, "Cannot allocate PipeWire DSD session"); return NULL; }
    if (s->error) {
        chilpa_pw_dsd_error(s, error, size);
        chilpa_pw_dsd_close(s);
        return NULL;
    }
    pw_thread_loop_lock(s->loop);
    s->rate = rate;
    s->interleave = interleave;
    uint32_t mask = interleave == 4 ? s->caps.be_rates : s->caps.le_rates;
    bool supported = false;
    for (unsigned i = 0; i < 4; i++)
        if (rate == (88200u << i) && (mask & (1u << i))) supported = true;
    if ((interleave != 4 && interleave != -4) || !supported)
        dsd_fail(s, -ENOTSUP, "DSD rate or U32 layout is not supported by this sink");
    if (!s->error) {
        s->capacity = rate / 2u;
        s->ring = malloc(s->capacity * DSD_FRAME_BYTES);
        if (!s->ring) dsd_fail(s, -ENOMEM, "Cannot allocate DSD ring");
    }
    if (!s->error) {
        int result = dsd_session_start_stream(s, &deadline);
        if (result < 0) dsd_fail(s, result, "Cannot connect native DSD stream");
    }
    pw_thread_loop_unlock(s->loop);
    if (s->error) {
        chilpa_pw_dsd_error(s, error, size);
        chilpa_pw_dsd_close(s);
        return NULL;
    }
    return s;
}

/* The route of this activation: format, links and buffers back, and the
 * stream streaming. A suspend only reaches a stream that is not running, so
 * once this one streams, nothing from the last deactivation can clear it. */
static bool route_confirmed(struct dsd_session *s)
{
    return dsd_route_ready(s) && s->buffer_count >= 2 && s->state == PW_STREAM_STATE_STREAMING;
}

/* After a suspend the format, the links and the buffers come back only once
 * the stream is active again. Waits for them, with the open deadline. A
 * suspend set off by the last deactivation may still be on its way, and the
 * route would look ready from before it: a roundtrip delivers it first. */
static int confirm_route(struct dsd_session *s)
{
    struct timespec deadline;
    pw_thread_loop_get_time(s->loop, &deadline, DSD_OPEN_NS);
    int result = dsd_roundtrip(s, &deadline);
    if (result < 0) dsd_fail(s, result, "Cannot confirm the native DSD route");
    while (!s->error && !route_confirmed(s)) {
        result = pw_thread_loop_timed_wait_full(s->loop, &deadline);
        if (result < 0 && !route_confirmed(s))
            dsd_fail(s, result, "DSD route did not return after the stream was suspended");
    }
    s->live = s->error == 0;
    return s->error;
}

static int activate(struct dsd_session *s)
{
    int result = pw_stream_set_active(s->stream, true);
    if (result < 0) dsd_fail(s, result, "Cannot activate native DSD stream");
    else s->active = true;
    return s->error ? s->error : confirm_route(s);
}

int64_t chilpa_pw_dsd_write(struct dsd_session *s, const uint8_t *bytes, size_t size)
{
    pw_thread_loop_lock(s->loop);
    int64_t result = s->error;
    /* Before activation the stream may be suspended; the ring takes the bytes
     * and activation confirms the route. */
    if (!result && (size % DSD_FRAME_BYTES || (s->live && !dsd_route_ready(s)))) result = -EPROTO;
    if (!result && s->xrun) { s->xrun = false; result = -EPIPE; }
    if (!result && !s->paused) {
        if (s->drain_requested && !s->drained) result = -EBUSY;
        else {
            s->draining = s->drain_requested = s->drained = false;
            result = dsd_ring_write(s, bytes, size / DSD_FRAME_BYTES);
            s->submitted += result;
            /* Prime a quarter second. Drain activates even a shorter track. */
            if (!s->active && s->write_at - s->read_at >= s->rate / 4u)
                if (activate(s) < 0) result = s->error;
        }
    }
    pw_thread_loop_unlock(s->loop);
    return result;
}

static uint64_t stream_delay(struct dsd_session *s)
{
    struct pw_time time = { 0 };
    if (pw_stream_get_time_n(s->stream, &time, sizeof(time)) >= 0) {
        uint64_t delay = time.queued + time.buffered;
        if (time.delay > 0 && time.rate.denom)
            delay += (uint64_t)time.delay * time.rate.num * s->rate / time.rate.denom;
        s->last_delay = SPA_MIN(delay, s->submitted);
    }
    /* PipeWire returns EIO outside STREAMING. Keep the last queue snapshot
     * across a pause rather than reporting those pending frames as played. */
    return s->last_delay;
}

int chilpa_pw_dsd_pause(struct dsd_session *s, bool paused)
{
    pw_thread_loop_lock(s->loop);
    if (!s->error && s->paused != paused) {
        if (paused) stream_delay(s);
        s->paused = paused;
        if (paused) s->live = false;
        if (s->active) {
            int result = pw_stream_set_active(s->stream, !paused);
            if (result < 0) dsd_fail(s, result, "Cannot pause native DSD stream");
            else if (!paused) confirm_route(s);
        }
    }
    int result = s->error;
    pw_thread_loop_unlock(s->loop);
    return result;
}

int chilpa_pw_dsd_flush(struct dsd_session *s)
{
    pw_thread_loop_lock(s->loop);
    int result = s->error;
    if (!result) {
        result = pw_stream_set_active(s->stream, false);
        if (result >= 0) result = pw_stream_flush(s->stream, false);
        if (result < 0) dsd_fail(s, result, "Cannot flush native DSD stream");
        s->active = s->live = s->draining = s->drain_requested = s->drained = s->xrun = false;
        s->read_at = s->write_at = 0;
        s->submitted = 0;
        s->last_delay = 0;
    }
    pw_thread_loop_unlock(s->loop);
    return result;
}

int chilpa_pw_dsd_drain(struct dsd_session *s)
{
    pw_thread_loop_lock(s->loop);
    if (!s->error && !s->draining) {
        s->draining = true;
        s->drained = false;
        s->settle_until = 0;
        if (!s->active) activate(s);
    }
    if (!s->error && s->write_at == s->read_at && !s->drain_requested) {
        s->drain_requested = true;
        s->drain_delay = 0;
        s->drain_num = s->drain_denom = 0;
        device_delay(s, &s->drain_delay, &s->drain_num, &s->drain_denom);
        int result = pw_stream_flush(s->stream, true);
        if (result < 0) dsd_fail(s, result, "Cannot drain native DSD stream");
    }
    int result = s->error ? s->error : (settled(s) ? 1 : 0);
    pw_thread_loop_unlock(s->loop);
    return result;
}

uint64_t chilpa_pw_dsd_held(struct dsd_session *s)
{
    pw_thread_loop_lock(s->loop);
    uint64_t held = s->write_at - s->read_at;
    if (!s->drained) held += stream_delay(s);
    held = SPA_MIN(held, s->submitted);
    pw_thread_loop_unlock(s->loop);
    return held;
}
