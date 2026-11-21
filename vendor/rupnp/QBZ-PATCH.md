# QBZ UPnP endpoint compatibility

Source: crates.io `rupnp` 3.0.0 (MIT / Apache-2.0); upstream repository and
commit are recorded in Cargo.toml and .cargo_vcs_info.json. All original files
are retained. The only implementation change is in src/service.rs:
`parse_service_endpoint` handles empty or missing-leading-slash service URLs.
An annotation in device.rs also silences the upstream unused presentation_url
field, whose warning is normally capped for registry dependencies.

Valid `PathAndQuery` values are returned by the original parser. The fallback
restores empty-path tolerance and normalizes rejected relative URLs into
root-relative request targets, reusing the current http parser so character
and length checks still apply. In 1.4.0 a relative endpoint was actually sent
without a leading slash; this patch deliberately uses valid origin-form instead.
It does not change endpoints that were valid in 2.1.0. Device fetching,
SSDP, SOAP, service selection and URLBase semantics are unchanged.

This replaces the workspace-wide http 1.4.0 pin from #745, retaining the
subsequent HTTP validation and HeaderMap fixes. Fixtures are synthetic; KEF
hardware confirmation remains required. Tests live in qbz-cast/tests:
`dlna_description_tolerance.rs` and `dlna_wire_compatibility.rs`.

The workspace uses a local Cargo patch, never a network git dependency. This
folder ships in the QBZ source archive; registry dependencies still come from
the locked release vendor archive for offline AUR/Gentoo builds.

Remove this patch once an upstream release provides the same compatibility
and passes those tests. This is not a general UPnP URL-resolution rewrite.
