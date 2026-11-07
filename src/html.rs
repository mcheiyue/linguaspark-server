use crate::AppError;
use html5ever::{QualName, local_name, ns, parse_fragment, serialize, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom, SerializableHandle};

pub struct HtmlPlan {
    source: String,
    pub nodes: Vec<String>,
}

impl HtmlPlan {
    pub fn new(source: &str) -> Result<Self, AppError> {
        let (_document, root) = parse(source)?;
        let nodes = text_nodes(&root)
            .iter()
            .filter_map(|node| match &node.data {
                NodeData::Text { contents } => Some(contents.borrow().to_string()),
                _ => None,
            })
            .collect();
        Ok(Self {
            source: source.into(),
            nodes,
        })
    }

    pub fn render(&self, nodes: &[String]) -> Result<String, AppError> {
        if nodes.len() != self.nodes.len() {
            return Err(AppError::InferenceError(
                "HTML text node count mismatch".into(),
            ));
        }
        if self.nodes == nodes {
            return Ok(self.source.clone());
        }
        let (_document, root) = parse(&self.source)?;
        let handles = text_nodes(&root);
        for (handle, translated) in handles.iter().zip(nodes) {
            if let NodeData::Text { contents } = &handle.data {
                *contents.borrow_mut() = translated.as_str().into();
            }
        }
        let mut output = Vec::new();
        serialize(
            &mut output,
            &SerializableHandle::from(root),
            Default::default(),
        )?;
        String::from_utf8(output).map_err(|error| AppError::TranslationError(error.to_string()))
    }
}

fn parse(source: &str) -> Result<(RcDom, Handle), AppError> {
    let dom = parse_fragment(
        RcDom::default(),
        Default::default(),
        QualName::new(None, ns!(html), local_name!(div)),
        Vec::new(),
        false,
    )
    .one(source);
    let root = dom
        .document
        .children
        .borrow()
        .first()
        .cloned()
        .ok_or_else(|| AppError::TranslationError("Missing HTML fragment root".into()))?;
    Ok((dom, root))
}

fn text_nodes(root: &Handle) -> Vec<Handle> {
    let mut pending = vec![root.clone()];
    let mut result = Vec::new();
    while let Some(node) = pending.pop() {
        match &node.data {
            NodeData::Element { name, attrs, .. } => {
                if matches!(
                    name.local.as_ref(),
                    "script" | "style" | "pre" | "code" | "textarea" | "svg" | "math" | "template"
                ) || attrs.borrow().iter().any(|attr| {
                    (attr.name.local == local_name!(translate)
                        && attr.value.eq_ignore_ascii_case("no"))
                        || (attr.name.local == local_name!(class)
                            && attr
                                .value
                                .split_ascii_whitespace()
                                .any(|class| class == "notranslate"))
                }) {
                    continue;
                }
            }
            NodeData::Text { .. } => result.push(node.clone()),
            NodeData::Document
            | NodeData::Doctype { .. }
            | NodeData::Comment { .. }
            | NodeData::ProcessingInstruction { .. } => {}
        }
        pending.extend(node.children.borrow().iter().rev().cloned());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_markup_and_excluded_content() {
        let source = "Root <p id='x'>Hello &amp; world <a href='/a?a=1&amp;b=2'>read</a></p><!--keep--><code>do not change</code><span translate='no'>Keep <b>this</b></span><p>after</p>";
        let plan = HtmlPlan::new(source).unwrap();
        assert_eq!(plan.nodes, ["Root ", "Hello & world ", "read", "after"]);
        assert_eq!(plan.render(&plan.nodes).unwrap(), source);
        let rendered = plan
            .render(&[
                "根 ".into(),
                "你好 & 世界 ".into(),
                "<img>".into(),
                "之后".into(),
            ])
            .unwrap();
        assert!(rendered.contains("href=\"/a?a=1&amp;b=2\">&lt;img&gt;</a>"));
        assert!(rendered.contains(
            "<code>do not change</code><span translate=\"no\">Keep <b>this</b></span><p>之后</p>"
        ));
        assert!(rendered.contains("你好 &amp; 世界"));
        assert!(rendered.contains("<!--keep-->"));
    }

    #[test]
    fn implicit_closing_does_not_hide_following_nodes() {
        let plan = HtmlPlan::new("<p translate='no'>Keep<p>Translate").unwrap();
        assert_eq!(plan.nodes, ["Translate"]);
    }
}
