use http::HeaderMap;

pub(crate) fn merge_request_headers(
    provider_headers: &HeaderMap,
    extra_headers: HeaderMap,
    default_headers: HeaderMap,
) -> HeaderMap {
    let mut headers = provider_headers.clone();
    headers.extend(extra_headers);
    for (name, value) in &default_headers {
        if let http::header::Entry::Vacant(entry) = headers.entry(name) {
            entry.insert(value.clone());
        }
    }
    headers
}
