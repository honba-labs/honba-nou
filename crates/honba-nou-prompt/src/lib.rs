//! Prompt templates and context serialization.
pub struct PromptTemplate {
    pub name: String,
    pub body: String,
}

impl PromptTemplate {
    pub fn render(&self, ctx: &str) -> String {
        self.body.replace("{{context}}", ctx)
    }
}
