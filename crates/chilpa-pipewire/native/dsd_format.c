/* SPDX-License-Identifier: MIT */
#include "dsd.h"
#include <string.h>

bool dsd_choice_contains(uint32_t type, const int32_t *v, uint32_t n, int32_t wanted)
{
    if (n == 0 || n > 128) return false;
    switch (type) {
    case SPA_CHOICE_None: return n == 1 && v[0] == wanted;
    case SPA_CHOICE_Enum:
        for (uint32_t i = 0; i < n; i++) if (v[i] == wanted) return true;
        return false;
    case SPA_CHOICE_Range:
        return n == 3 && v[1] <= v[2] && wanted >= v[1] && wanted <= v[2];
    case SPA_CHOICE_Step:
        return n == 4 && v[1] <= v[2] && v[3] > 0 && wanted >= v[1] &&
            wanted <= v[2] && ((int64_t)wanted - v[1]) % v[3] == 0;
    default: return false;
    }
}

static bool property_contains(const struct spa_pod *pod, uint32_t key,
        uint32_t value_type, int32_t wanted)
{
    const struct spa_pod_prop *p = spa_pod_find_prop(pod, NULL, key);
    if (!p) return false;
    const struct spa_pod *v = &p->value;
    if (v->type == value_type && v->size == sizeof(int32_t)) {
        int32_t actual;
        memcpy(&actual, SPA_POD_BODY(v), sizeof(actual));
        return actual == wanted;
    }
    if (!spa_pod_is_choice(v) || SPA_POD_CHOICE_VALUE_TYPE(v) != value_type ||
            SPA_POD_CHOICE_VALUE_SIZE(v) != sizeof(int32_t)) return false;
    return dsd_choice_contains(SPA_POD_CHOICE_TYPE(v), SPA_POD_CHOICE_VALUES(v),
            SPA_POD_CHOICE_N_VALUES(v), wanted);
}

static bool stereo_positions(const struct spa_pod *pod)
{
    const struct spa_pod_prop *p = spa_pod_find_prop(pod, NULL, SPA_FORMAT_AUDIO_position);
    uint32_t positions[2];
    return p && spa_pod_is_array(&p->value) && SPA_POD_ARRAY_N_VALUES(&p->value) == 2 &&
        spa_pod_copy_array(&p->value, SPA_TYPE_Id, positions, 2) == 2 &&
        positions[0] == SPA_AUDIO_CHANNEL_FL && positions[1] == SPA_AUDIO_CHANNEL_FR;
}

static bool is_dsd(const struct spa_pod *pod)
{
    uint32_t type, subtype;
    return pod && spa_format_parse(pod, &type, &subtype) >= 0 &&
        type == SPA_MEDIA_TYPE_audio && subtype == SPA_MEDIA_SUBTYPE_dsd;
}

void dsd_collect_format(const struct spa_pod *pod, struct dsd_caps *caps)
{
    if (!is_dsd(pod) || !stereo_positions(pod) ||
        !property_contains(pod, SPA_FORMAT_AUDIO_channels, SPA_TYPE_Int, 2) ||
        !property_contains(pod, SPA_FORMAT_AUDIO_bitorder, SPA_TYPE_Id, SPA_PARAM_BITORDER_msb)) return;
    /* Keep each tuple intact; never multiply rates from a BE pod by LE caps. */
    for (unsigned i = 0; i < 4; i++) {
        if (!property_contains(pod, SPA_FORMAT_AUDIO_rate, SPA_TYPE_Int, 352800u << i)) continue;
        if (property_contains(pod, SPA_FORMAT_AUDIO_interleave, SPA_TYPE_Int, 4)) caps->be_rates |= 1u << i;
        if (property_contains(pod, SPA_FORMAT_AUDIO_interleave, SPA_TYPE_Int, -4)) caps->le_rates |= 1u << i;
    }
}

bool dsd_format_matches(const struct spa_pod *pod, uint32_t rate, int32_t interleave)
{
    if (rate > UINT32_MAX / 4u) return false;
    struct spa_audio_info_dsd f = { 0 };
    return is_dsd(pod) && spa_format_audio_dsd_parse(pod, &f) >= 0 &&
        f.rate == rate * 4u && f.channels == 2 && f.interleave == interleave &&
        f.bitorder == SPA_PARAM_BITORDER_msb && !(f.flags & SPA_AUDIO_FLAG_UNPOSITIONED) &&
        f.position[0] == SPA_AUDIO_CHANNEL_FL && f.position[1] == SPA_AUDIO_CHANNEL_FR;
}

size_t dsd_ring_write(struct dsd_session *s, const uint8_t *bytes, size_t frames)
{
    size_t available = s->capacity - (s->write_at - s->read_at);
    size_t count = SPA_MIN(frames, available);
    size_t offset = s->write_at % s->capacity;
    size_t first = SPA_MIN(count, s->capacity - offset);
    memcpy(s->ring + offset * DSD_FRAME_BYTES, bytes, first * DSD_FRAME_BYTES);
    memcpy(s->ring, bytes + first * DSD_FRAME_BYTES, (count - first) * DSD_FRAME_BYTES);
    s->write_at += count;
    return count;
}

size_t dsd_ring_read(struct dsd_session *s, uint8_t *bytes, size_t frames)
{
    size_t count = SPA_MIN(frames, s->write_at - s->read_at);
    size_t offset = s->read_at % s->capacity;
    size_t first = SPA_MIN(count, s->capacity - offset);
    memcpy(bytes, s->ring + offset * DSD_FRAME_BYTES, first * DSD_FRAME_BYTES);
    memcpy(bytes + first * DSD_FRAME_BYTES, s->ring, (count - first) * DSD_FRAME_BYTES);
    s->read_at += count;
    return count;
}
