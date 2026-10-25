/* SPDX-License-Identifier: MIT */
/* Protocol fixtures used by the Rust tests, including an isolated graph sink. */
#include "dsd.h"
#include <stdlib.h>
#include <string.h>
#include <errno.h>

struct test_sink {
    struct pw_thread_loop *loop;
    struct pw_stream *stream;
    struct spa_hook listener;
    uint8_t *bytes;
    size_t used, capacity;
    bool failed;
};

static void test_process(void *data)
{
    struct test_sink *s = data;
    struct pw_buffer *b = pw_stream_dequeue_buffer(s->stream);
    if (!b) return;
    struct spa_data *d = b->buffer->datas;
    if (b->buffer->n_datas != 1 || !d->data || !d->chunk ||
            d->chunk->offset > d->maxsize || d->chunk->size > d->maxsize - d->chunk->offset ||
            d->chunk->size > s->capacity - s->used) s->failed = true;
    else {
        memcpy(s->bytes + s->used, (uint8_t *)d->data + d->chunk->offset, d->chunk->size);
        s->used += d->chunk->size;
    }
    pw_stream_queue_buffer(s->stream, b);
}

static void test_state(void *data, enum pw_stream_state old, enum pw_stream_state state, const char *error)
{
    (void)old; (void)error;
    if (state == PW_STREAM_STATE_ERROR) ((struct test_sink *)data)->failed = true;
}

static const struct pw_stream_events test_events = {
    PW_VERSION_STREAM_EVENTS, .process = test_process, .state_changed = test_state,
};

void *chilpa_pw_dsd_test_sink_new(const char *name, uint32_t rate, int32_t interleave)
{
    pw_init(NULL, NULL);
    struct test_sink *s = calloc(1, sizeof(*s));
    if (!s) return NULL;
    s->capacity = 8u * 1024u * 1024u;
    s->bytes = malloc(s->capacity);
    s->loop = pw_thread_loop_new("chilpa-dsd-fixture", NULL);
    if (!s->bytes || !s->loop) goto fail;
    struct pw_properties *props = pw_properties_new(PW_KEY_MEDIA_CLASS, "Audio/Sink",
        PW_KEY_NODE_NAME, name, PW_KEY_DEVICE_API, "alsa", "node.virtual", "false",
        "node.pause-on-idle", "false", NULL);
    s->stream = pw_stream_new_simple(pw_thread_loop_get_loop(s->loop), "DSD fixture",
        props, &test_events, s);
    if (!s->stream) goto fail;
    struct spa_audio_info_dsd info = { .rate = rate * 4u, .channels = 2,
        .bitorder = SPA_PARAM_BITORDER_msb, .interleave = interleave,
        .position = { SPA_AUDIO_CHANNEL_FL, SPA_AUDIO_CHANNEL_FR } };
    uint8_t storage[512];
    struct spa_pod_builder builder = SPA_POD_BUILDER_INIT(storage, sizeof(storage));
    const struct spa_pod *format = spa_format_audio_dsd_build(&builder, SPA_PARAM_EnumFormat, &info);
    if (pw_stream_connect(s->stream, PW_DIRECTION_INPUT, PW_ID_ANY,
            PW_STREAM_FLAG_MAP_BUFFERS | PW_STREAM_FLAG_NO_CONVERT, &format, 1) < 0) goto fail;
    if (pw_thread_loop_start(s->loop) < 0) goto fail;
    return s;
fail:
    if (s->stream) pw_stream_destroy(s->stream);
    if (s->loop) pw_thread_loop_destroy(s->loop);
    free(s->bytes);
    free(s);
    return NULL;
}

int64_t chilpa_pw_dsd_test_sink_copy(void *handle, uint8_t *bytes, size_t size)
{
    struct test_sink *s = handle;
    pw_thread_loop_lock(s->loop);
    int64_t result = s->failed ? -EIO : (int64_t)SPA_MIN(size, s->used);
    if (result >= 0) memcpy(bytes, s->bytes, result);
    pw_thread_loop_unlock(s->loop);
    return result;
}

void chilpa_pw_dsd_test_sink_close(void *handle)
{
    struct test_sink *s = handle;
    pw_thread_loop_stop(s->loop);
    pw_stream_destroy(s->stream);
    pw_thread_loop_destroy(s->loop);
    free(s->bytes);
    free(s);
}

uint32_t chilpa_pw_dsd_test_format(int32_t interleave, bool lsb, bool swapped,
        uint32_t choice, const int32_t *rates, uint32_t count, uint32_t channels,
        uint32_t exact_rate)
{
    if (count > 4) return 0;
    uint8_t storage[1024];
    struct spa_pod_builder b = SPA_POD_BUILDER_INIT(storage, sizeof(storage));
    struct spa_pod_frame object, values;
    uint32_t positions[2] = { swapped ? SPA_AUDIO_CHANNEL_FR : SPA_AUDIO_CHANNEL_FL,
                             swapped ? SPA_AUDIO_CHANNEL_FL : SPA_AUDIO_CHANNEL_FR };
    spa_pod_builder_push_object(&b, &object, SPA_TYPE_OBJECT_Format, SPA_PARAM_EnumFormat);
    spa_pod_builder_add(&b,
        SPA_FORMAT_mediaType, SPA_POD_Id(SPA_MEDIA_TYPE_audio),
        SPA_FORMAT_mediaSubtype, SPA_POD_Id(SPA_MEDIA_SUBTYPE_dsd),
        SPA_FORMAT_AUDIO_bitorder, SPA_POD_Id(lsb ? SPA_PARAM_BITORDER_lsb : SPA_PARAM_BITORDER_msb),
        SPA_FORMAT_AUDIO_interleave, SPA_POD_Int(interleave),
        SPA_FORMAT_AUDIO_channels, SPA_POD_Int(channels),
        SPA_FORMAT_AUDIO_position, SPA_POD_Array(sizeof(uint32_t), SPA_TYPE_Id, 2, positions), 0);
    spa_pod_builder_prop(&b, SPA_FORMAT_AUDIO_rate, 0);
    if (choice == SPA_CHOICE_None) {
        if (count) spa_pod_builder_int(&b, rates[0]);
        else spa_pod_builder_none(&b);
    } else {
        spa_pod_builder_push_choice(&b, &values, choice, 0);
        for (uint32_t i = 0; i < count; i++) spa_pod_builder_int(&b, rates[i]);
        spa_pod_builder_pop(&b, &values);
    }
    struct spa_pod *pod = spa_pod_builder_pop(&b, &object);
    struct dsd_caps caps = { 0 };
    dsd_collect_format(pod, &caps);
    return caps.be_rates | (caps.le_rates << 8) |
        (dsd_format_matches(pod, exact_rate, interleave) ? 1u << 16 : 0);
}

bool chilpa_pw_dsd_test_ring(void)
{
    struct dsd_session s = { .capacity = 7, .read_at = (1ull << 32) - 3, .write_at = (1ull << 32) - 3 };
    uint8_t ring[7 * DSD_FRAME_BYTES], source[23 * DSD_FRAME_BYTES], output[23 * DSD_FRAME_BYTES];
    s.ring = ring;
    for (unsigned i = 0; i < sizeof(source); i++) source[i] = (i * 71u + i / 8u) & 255u;
    memset(output, 0, sizeof(output));
    size_t written = 0, read = 0;
    for (unsigned turn = 0; turn < 40 && read < 23; turn++) {
        written += dsd_ring_write(&s, source + written * DSD_FRAME_BYTES, SPA_MIN(5u, 23u - written));
        read += dsd_ring_read(&s, output + read * DSD_FRAME_BYTES, SPA_MIN(3u, 23u - read));
    }
    return written == 23 && read == 23 && s.read_at == s.write_at &&
        memcmp(source, output, sizeof(source)) == 0;
}
