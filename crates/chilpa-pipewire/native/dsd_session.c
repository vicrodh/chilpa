/* SPDX-License-Identifier: MIT */
#include "dsd.h"
#include <errno.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <spa/utils/string.h>

static pthread_once_t initialized = PTHREAD_ONCE_INIT;
static void initialize(void) { pw_init(NULL, NULL); }

void dsd_fail(struct dsd_session *s, int error, const char *message)
{
    if (s->closing || s->error) return;
    s->error = error < 0 ? error : -EIO;
    snprintf(s->message, sizeof(s->message), "%s", message ? message : spa_strerror(s->error));
    if (s->loop) pw_thread_loop_signal(s->loop, false);
}

static bool property_is(const struct spa_dict *props, const char *key, const char *value)
{
    const char *s = props ? spa_dict_lookup(props, key) : NULL;
    return s && strcmp(s, value) == 0;
}

static uint64_t serial_of(const struct spa_dict *props)
{
    const char *v = props ? spa_dict_lookup(props, PW_KEY_OBJECT_SERIAL) : NULL;
    char *end;
    if (!v || !*v || *v == '-') return 0;
    errno = 0;
    unsigned long long serial = strtoull(v, &end, 10);
    return errno || *end ? 0 : serial;
}

static void core_done(void *data, uint32_t id, int seq)
{
    struct dsd_session *s = data;
    if (id == PW_ID_CORE) s->done_seq = seq;
    pw_thread_loop_signal(s->loop, false);
}

static void core_info(void *data, const struct pw_core_info *info)
{
    struct dsd_session *s = data;
    s->core_cookie = info->cookie;
    if (s->expected_serial && s->expected_cookie != info->cookie)
        dsd_fail(s, -ENODEV, "PipeWire server instance changed while DSD was suspended");
}

static void core_error(void *data, uint32_t id, int seq, int result, const char *message)
{
    (void)id; (void)seq;
    dsd_fail(data, result, message);
}

static const struct pw_core_events core_events = {
    PW_VERSION_CORE_EVENTS, .info = core_info, .done = core_done, .error = core_error,
};

static void target_info(void *data, const struct pw_node_info *info)
{
    struct dsd_session *s = data;
    if ((info->change_mask & PW_NODE_CHANGE_MASK_PROPS) && info->props) {
        const char *virtual = spa_dict_lookup(info->props, "node.virtual");
        s->physical = property_is(info->props, PW_KEY_DEVICE_API, "alsa") &&
            !(virtual && spa_atob(virtual)) &&
            property_is(info->props, PW_KEY_MEDIA_CLASS, "Audio/Sink") &&
            property_is(info->props, PW_KEY_NODE_NAME, s->name) &&
            serial_of(info->props) == s->caps.serial;
        if (s->opened && !s->physical)
            dsd_fail(s, -ENODEV, "DSD target identity or ALSA route changed");
    }
    if (info->state == PW_NODE_STATE_ERROR)
        dsd_fail(s, -ENODEV, info->error ? info->error : "DSD target failed");
    pw_thread_loop_signal(s->loop, false);
}

static void target_param(void *data, int seq, uint32_t id, uint32_t index,
        uint32_t next, const struct spa_pod *param)
{
    (void)seq; (void)index; (void)next;
    struct dsd_session *s = data;
    if (id != SPA_PARAM_EnumFormat) return;
    if (++s->param_count > 256) {
        dsd_fail(s, -E2BIG, "DSD target returned too many formats");
        return;
    }
    dsd_collect_format(param, &s->caps);
}

static const struct pw_node_events target_events = {
    PW_VERSION_NODE_EVENTS, .info = target_info, .param = target_param,
};

bool dsd_route_ready(struct dsd_session *s)
{
    if (s->error || !s->physical || !s->format_valid) return false;
    bool linked = false;
    for (unsigned i = 0; i < DSD_MAX_LINKS; i++) {
        if (!s->links[i].proxy) continue;
        if (!s->links[i].valid) return false;
        linked = true;
    }
    return linked;
}

static void link_info(void *data, const struct pw_link_info *info)
{
    struct dsd_link *link = data;
    struct dsd_session *s = link->session;
    uint32_t own = pw_stream_get_node_id(s->stream);
    if (info->output_node_id != own || info->input_node_id != s->target_id) {
        dsd_fail(s, -EPROTO, "DSD stream was linked to a different target");
        return;
    }
    if (info->change_mask & PW_LINK_CHANGE_MASK_FORMAT) {
        link->format_valid = dsd_format_matches(info->format, s->rate, s->interleave);
        if (info->format && !link->format_valid)
            dsd_fail(s, -EPROTO, "DSD link negotiated a different format");
    }
    link->valid = link->format_valid && info->state >= PW_LINK_STATE_PAUSED;
    if (info->state == PW_LINK_STATE_ERROR || info->state == PW_LINK_STATE_UNLINKED ||
            (s->opened && !link->valid))
        dsd_fail(s, -ENODEV, info->error ? info->error : "DSD link was lost");
    pw_thread_loop_signal(s->loop, false);
}

static const struct pw_link_events link_events = {
    PW_VERSION_LINK_EVENTS, .info = link_info,
};

static void registry_global(void *data, uint32_t id, uint32_t permissions,
        const char *type, uint32_t version, const struct spa_dict *props)
{
    (void)permissions;
    struct dsd_session *s = data;
    if (strcmp(type, PW_TYPE_INTERFACE_Node) == 0 &&
            property_is(props, PW_KEY_NODE_NAME, s->name) &&
            property_is(props, PW_KEY_MEDIA_CLASS, "Audio/Sink")) {
        uint64_t serial = serial_of(props);
        if (s->target || !serial || (s->expected_serial && s->expected_serial != serial)) {
            dsd_fail(s, -ENODEV, "DSD target identity is ambiguous or changed");
            return;
        }
        s->target_id = id;
        s->caps.serial = serial;
        s->target = pw_registry_bind(s->registry, id, type, SPA_MIN(version, (uint32_t)PW_VERSION_NODE), 0);
        if (!s->target) { dsd_fail(s, -errno, "Cannot inspect DSD target"); return; }
        pw_node_add_listener(s->target, &s->node_listener, &target_events, s);
    } else if (s->stream && strcmp(type, PW_TYPE_INTERFACE_Link) == 0) {
        const char *out = props ? spa_dict_lookup(props, PW_KEY_LINK_OUTPUT_NODE) : NULL;
        uint32_t own = pw_stream_get_node_id(s->stream);
        if (!out || own == PW_ID_ANY || strtoul(out, NULL, 10) != own) return;
        for (unsigned i = 0; i < DSD_MAX_LINKS; i++) {
            struct dsd_link *link = &s->links[i];
            if (link->proxy) continue;
            link->session = s;
            link->id = id;
            link->proxy = pw_registry_bind(s->registry, id, type, SPA_MIN(version, (uint32_t)PW_VERSION_LINK), 0);
            if (!link->proxy) { dsd_fail(s, -errno, "Cannot inspect DSD link"); return; }
            pw_link_add_listener(link->proxy, &link->listener, &link_events, link);
            if (s->opened) dsd_fail(s, -EPROTO, "DSD stream gained an unexpected link");
            return;
        }
        dsd_fail(s, -E2BIG, "DSD stream has too many links");
    }
}

static void registry_remove(void *data, uint32_t id)
{
    struct dsd_session *s = data;
    if (id == s->target_id) dsd_fail(s, -ENODEV, "DSD target was removed");
    for (unsigned i = 0; i < DSD_MAX_LINKS; i++) {
        struct dsd_link *link = &s->links[i];
        if (!link->proxy || link->id != id) continue;
        link->valid = false;
        if (s->opened) dsd_fail(s, -ENODEV, "DSD link was removed");
        spa_hook_remove(&link->listener);
        pw_proxy_destroy((struct pw_proxy *)link->proxy);
        link->proxy = NULL;
    }
}

static const struct pw_registry_events registry_events = {
    PW_VERSION_REGISTRY_EVENTS, .global = registry_global, .global_remove = registry_remove,
};

int dsd_roundtrip(struct dsd_session *s, const struct timespec *deadline)
{
    s->sync_seq = pw_core_sync(s->core, PW_ID_CORE, s->sync_seq);
    if (s->sync_seq < 0) return s->sync_seq;
    while (s->done_seq != s->sync_seq && !s->error) {
        int result = pw_thread_loop_timed_wait_full(s->loop, deadline);
        if (result < 0) { dsd_fail(s, result, "PipeWire DSD negotiation timed out"); break; }
    }
    return s->error;
}

struct dsd_session *dsd_session_new(const char *name, uint64_t serial, uint32_t cookie, struct timespec *deadline)
{
    pthread_once(&initialized, initialize);
    struct dsd_session *s = calloc(1, sizeof(*s));
    if (!s) return NULL;
    s->name = name;
    s->expected_serial = serial;
    s->expected_cookie = cookie;
    s->target_id = PW_ID_ANY;
    s->done_seq = -1;
    s->loop = pw_thread_loop_new("chilpa-dsd", NULL);
    if (!s->loop) { s->error = -ENOMEM; return s; }
    pw_thread_loop_get_time(s->loop, deadline, DSD_OPEN_NS);
    s->context = pw_context_new(pw_thread_loop_get_loop(s->loop), NULL, 0);
    if (!s->context) { s->error = -ENOMEM; return s; }
    s->core = pw_context_connect(s->context, NULL, 0);
    if (!s->core) { s->error = errno ? -errno : -EIO; return s; }
    pw_core_add_listener(s->core, &s->core_listener, &core_events, s);
    s->registry = pw_core_get_registry(s->core, PW_VERSION_REGISTRY, 0);
    if (!s->registry) { s->error = -ENOMEM; return s; }
    pw_registry_add_listener(s->registry, &s->registry_listener, &registry_events, s);
    int result = pw_thread_loop_start(s->loop);
    if (result < 0) { s->error = result; return s; }
    s->started_loop = true;
    pw_thread_loop_lock(s->loop);
    /* Registry, bound-node info, then the explicit enumeration share one deadline. */
    if (dsd_roundtrip(s, deadline) == 0 && !s->target)
        dsd_fail(s, -ENODEV, "PipeWire has no DSD target with this identity");
    if (!s->error && dsd_roundtrip(s, deadline) == 0 && !s->physical)
        dsd_fail(s, -ENOTSUP, "DSD target is not a physical ALSA sink");
    if (!s->error) {
        result = pw_node_enum_params(s->target, 0, SPA_PARAM_EnumFormat, 0, 256, NULL);
        if (result < 0) dsd_fail(s, result, "Cannot enumerate DSD target formats");
        else dsd_roundtrip(s, deadline);
    }
    pw_thread_loop_unlock(s->loop);
    return s;
}

void chilpa_pw_dsd_error(struct dsd_session *s, char *error, size_t size)
{
    if (s && s->started_loop) pw_thread_loop_lock(s->loop);
    snprintf(error, size, "%s", s && s->message[0] ? s->message :
            spa_strerror(s ? s->error : -ENOMEM));
    if (s && s->started_loop) pw_thread_loop_unlock(s->loop);
}

void chilpa_pw_dsd_close(struct dsd_session *s)
{
    if (!s) return;
    if (s->started_loop) pw_thread_loop_lock(s->loop);
    s->closing = true;
    s->opened = false;
    if (s->stream) {
        spa_hook_remove(&s->stream_listener);
        pw_stream_destroy(s->stream);
    }
    for (unsigned i = 0; i < DSD_MAX_LINKS; i++) {
        if (!s->links[i].proxy) continue;
        spa_hook_remove(&s->links[i].listener);
        pw_proxy_destroy((struct pw_proxy *)s->links[i].proxy);
    }
    if (s->target) { spa_hook_remove(&s->node_listener); pw_proxy_destroy((struct pw_proxy *)s->target); }
    if (s->registry) { spa_hook_remove(&s->registry_listener); pw_proxy_destroy((struct pw_proxy *)s->registry); }
    if (s->core) { spa_hook_remove(&s->core_listener); pw_core_disconnect(s->core); }
    if (s->started_loop) {
        pw_thread_loop_unlock(s->loop);
        pw_thread_loop_stop(s->loop);
    }
    if (s->context) pw_context_destroy(s->context);
    if (s->loop) pw_thread_loop_destroy(s->loop);
    free(s->ring);
    free(s);
}

int chilpa_pw_dsd_query(const char *name, struct dsd_caps *caps, char *error, size_t size)
{
    struct timespec deadline;
    struct dsd_session *s = dsd_session_new(name, 0, 0, &deadline);
    int result = s ? s->error : -ENOMEM;
    if (!result) *caps = s->caps;
    else chilpa_pw_dsd_error(s, error, size);
    chilpa_pw_dsd_close(s);
    return result;
}

uint64_t chilpa_pw_dsd_serial(struct dsd_session *s) { return s->caps.serial; }
uint32_t chilpa_pw_dsd_cookie(struct dsd_session *s) { return s->core_cookie; }
