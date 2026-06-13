use super::*;

pub(super) async fn render_page(url: &str, timeout_secs: u64) -> Result<RenderedPage, String> {
    render_page_with_user_agent(url, timeout_secs, SEARCH_USER_AGENT).await
}

pub(super) async fn render_page_with_user_agent(
    url: &str,
    timeout_secs: u64,
    user_agent_value: &str,
) -> Result<RenderedPage, String> {
    let url = Url::parse(url).map_err(|err| format!("invalid url: {err}"))?;
    let client = ObscuraHttpClient::with_full_options(Arc::new(CookieJar::new()), None, false);
    {
        let mut user_agent = client.user_agent.write().await;
        *user_agent = user_agent_value.to_string();
    }
    let response = timeout(Duration::from_secs(timeout_secs), client.fetch(&url))
        .await
        .map_err(|_| format!("timed out after {timeout_secs}s"))?;
    let response = response.map_err(|err| format!("fetch failed: {err}"))?;
    if !(200..300).contains(&response.status) {
        return Err(format!("http status {}", response.status));
    }

    let html = response.text();
    let dom = parse_html(&html);
    let links = extract_links(&dom, Some(&response.url));
    let text = extract_readable_text(&dom);
    let title = extract_title(&dom).unwrap_or_else(|| response.url.to_string());
    Ok(RenderedPage {
        url: response.url.to_string(),
        title,
        text,
        links,
    })
}

pub(super) fn extract_title(dom: &DomTree) -> Option<String> {
    dom.query_selector("title")
        .ok()
        .flatten()
        .and_then(|title| clean_title(&dom.text_content(title)))
}

pub(super) fn extract_links(dom: &DomTree, base_url: Option<&Url>) -> Vec<RenderedLink> {
    let mut links = Vec::new();
    for link_id in dom.query_selector_all("a").unwrap_or_default() {
        let Some(node) = dom.get_node(link_id) else {
            continue;
        };
        let Some(href) = node.get_attribute("href") else {
            continue;
        };
        let href = normalize_link_href(base_url, href);
        let text = clean_text(&dom.text_content(link_id));
        if let Some(href) = href
            && !text.is_empty()
        {
            links.push(RenderedLink { href, text });
        }
    }
    links
}

pub(super) fn extract_readable_text(dom: &DomTree) -> String {
    dom.query_selector("body")
        .ok()
        .flatten()
        .map(|body| {
            let mut output = String::new();
            collect_readable_text(dom, body, &mut output);
            clean_text(&output)
        })
        .unwrap_or_default()
}

pub(super) fn collect_readable_text(dom: &DomTree, node_id: NodeId, output: &mut String) {
    let Some(node) = dom.get_node(node_id) else {
        return;
    };
    match &node.data {
        NodeData::Text { contents } => {
            let trimmed = contents.trim();
            if !trimmed.is_empty() {
                output.push_str(trimmed);
                output.push(' ');
            }
        }
        NodeData::Element { name, .. } => {
            let tag = name.local.as_ref();
            if matches!(
                tag,
                "script" | "style" | "nav" | "header" | "footer" | "aside"
            ) {
                return;
            }
            let block = is_block_element(tag);
            if block {
                output.push('\n');
            }
            for child in dom.children(node_id) {
                collect_readable_text(dom, child, output);
            }
            if block {
                output.push('\n');
            }
        }
        _ => {
            for child in dom.children(node_id) {
                collect_readable_text(dom, child, output);
            }
        }
    }
}

pub(super) fn is_block_element(tag: &str) -> bool {
    matches!(
        tag,
        "article"
            | "blockquote"
            | "br"
            | "dd"
            | "div"
            | "dl"
            | "dt"
            | "figcaption"
            | "figure"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "hr"
            | "li"
            | "main"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "tr"
            | "ul"
    )
}
