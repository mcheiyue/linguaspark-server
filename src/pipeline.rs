use crate::{AppError, html::HtmlPlan, text::TextPlan};

pub struct Document {
    html: Option<HtmlPlan>,
    plans: Vec<TextPlan>,
}

impl Document {
    pub fn new(input: &str, html: bool) -> Result<Self, AppError> {
        let markup = if html {
            Some(HtmlPlan::new(input)?)
        } else {
            None
        };
        let plans = match &markup {
            Some(document) => document
                .nodes
                .iter()
                .map(|node| TextPlan::new(node))
                .collect(),
            None => vec![TextPlan::new(input)],
        };
        Ok(Self {
            html: markup,
            plans,
        })
    }

    pub fn segments(&self) -> impl Iterator<Item = &String> {
        self.plans.iter().flat_map(TextPlan::segments)
    }

    pub fn render(&self, results: &[String]) -> Result<String, AppError> {
        let mut offset = 0;
        let mut nodes = Vec::with_capacity(self.plans.len());
        for plan in &self.plans {
            let end = offset + plan.segments().len();
            let slice = results
                .get(offset..end)
                .ok_or_else(|| AppError::InferenceError("Missing document segments".into()))?;
            nodes.push(plan.render(slice)?);
            offset = end;
        }
        if offset != results.len() {
            return Err(AppError::InferenceError("Extra document segments".into()));
        }
        match &self.html {
            Some(html) => html.render(&nodes),
            None => Ok(nodes.concat()),
        }
    }
}
