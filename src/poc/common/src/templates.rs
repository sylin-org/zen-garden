//! Guidance template engine
//!
//! Simple template system for rendering guidance markdown with:
//! - Variable substitution: `{{variable}}` or `{{variable|default}}`
//! - Conditionals: `{{#if var}}...{{/if}}`, `{{#if !var}}...{{/if}}`
//! - Equality checks: `{{#if var=value}}...{{/if}}`
//! - Else blocks: `{{#if var}}...{{#else}}...{{/if}}`
//! - Nesting supported
//!
//! ## Example
//!
//! ```rust
//! use garden_common::templates::{Template, render_template};
//!
//! let mut ctx = Template::new();
//! ctx.set("name", "pihole");
//! ctx.set("static-ip", "192.168.1.240");
//!
//! let template = r#"
//! {{#if static-ip}}
//! DNS Server: {{static-ip}}
//! {{#else}}
//! Run: ping {{name}}
//! {{/if}}
//! "#;
//!
//! let rendered = render_template(template, &ctx);
//! assert!(rendered.contains("192.168.1.240"));
//! ```

use std::collections::HashMap;

/// Template rendering context with variable values
#[derive(Debug, Clone, Default)]
pub struct Template {
    variables: HashMap<String, String>,
}

impl Template {
    /// Create a new empty context
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a variable value
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.variables.insert(key.into(), value.into());
    }

    /// Set a variable only if the value is Some
    pub fn set_opt(&mut self, key: impl Into<String>, value: Option<impl Into<String>>) {
        if let Some(v) = value {
            self.variables.insert(key.into(), v.into());
        }
    }

    /// Get a variable value
    pub fn get(&self, key: &str) -> Option<&str> {
        self.variables.get(key).map(|s| s.as_str())
    }

    /// Check if a variable is set and non-empty (truthy)
    pub fn is_truthy(&self, key: &str) -> bool {
        self.variables
            .get(key)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }

    /// Check if a variable equals a specific value
    pub fn equals(&self, key: &str, value: &str) -> bool {
        self.variables.get(key).map(|v| v == value).unwrap_or(false)
    }
}

/// Render a template string with the given context
///
/// Supports:
/// - `{{variable}}` - Simple substitution
/// - `{{variable|default}}` - Substitution with fallback
/// - `{{#if var}}...{{/if}}` - Conditional block
/// - `{{#if !var}}...{{/if}}` - Negated conditional
/// - `{{#if var=value}}...{{/if}}` - Equality check
/// - `{{#if var}}...{{#else}}...{{/if}}` - If-else
/// - Nested conditionals
pub fn render_template(template: &str, ctx: &Template) -> String {
    let mut result = String::with_capacity(template.len());
    let mut pos = 0;

    while pos < template.len() {
        // Look for {{
        if template[pos..].starts_with("{{") {
            pos += 2; // skip {{

            // Find closing }}
            let tag_start = pos;
            while pos < template.len() && !template[pos..].starts_with("}}") {
                pos += 1;
            }
            let tag = template[tag_start..pos].trim();
            if template[pos..].starts_with("}}") {
                pos += 2; // skip }}
            }

            if let Some(directive) = tag.strip_prefix("#if ") {
                // Conditional block - find matching {{/if}}
                let remaining = &template[pos..];
                let (if_content, else_content, consumed) = extract_if_block(remaining);

                let condition_met = evaluate_condition(directive.trim(), ctx);

                if condition_met {
                    result.push_str(&render_template(&if_content, ctx));
                } else if let Some(else_body) = else_content {
                    result.push_str(&render_template(&else_body, ctx));
                }

                pos += consumed;
            } else if tag == "/if" || tag == "#else" {
                // These are handled by extract_if_block, shouldn't reach here at top level
                // If we do, it's malformed - just skip
            } else {
                // Variable substitution
                let (var_name, default_value) = if let Some((name, default)) = tag.split_once('|') {
                    (name.trim(), Some(default.trim()))
                } else {
                    (tag, None)
                };

                if let Some(value) = ctx.get(var_name) {
                    result.push_str(value);
                } else if let Some(default) = default_value {
                    result.push_str(default);
                }
                // If no value and no default, output nothing
            }
        } else {
            // Regular character - find next {{ or end
            let next_tag = template[pos..].find("{{").unwrap_or(template.len() - pos);
            result.push_str(&template[pos..pos + next_tag]);
            pos += next_tag;
        }
    }

    result
}

/// Evaluate a condition expression
///
/// Supports:
/// - `var` - truthy check
/// - `!var` - negated truthy check
/// - `var=value` - equality check
fn evaluate_condition(expr: &str, ctx: &Template) -> bool {
    let expr = expr.trim();

    if let Some(var) = expr.strip_prefix('!') {
        // Negated condition
        !ctx.is_truthy(var.trim())
    } else if let Some((var, value)) = expr.split_once('=') {
        // Equality check
        ctx.equals(var.trim(), value.trim())
    } else {
        // Simple truthy check
        ctx.is_truthy(expr)
    }
}

/// Extract if block content, handling nesting
///
/// Returns (if_content, else_content, bytes_consumed)
fn extract_if_block(template: &str) -> (String, Option<String>, usize) {
    let mut depth = 1;
    let mut if_content = String::new();
    let mut else_content: Option<String> = None;
    let mut in_else = false;
    let mut current = String::new();
    let mut pos = 0;

    while pos < template.len() {
        if template[pos..].starts_with("{{") {
            pos += 2; // skip {{

            // Find closing }}
            let tag_start = pos;
            while pos < template.len() && !template[pos..].starts_with("}}") {
                pos += 1;
            }
            let tag = &template[tag_start..pos];
            if template[pos..].starts_with("}}") {
                pos += 2; // skip }}
            }

            let tag_trimmed = tag.trim();

            if tag_trimmed.starts_with("#if ") {
                depth += 1;
                current.push_str("{{");
                current.push_str(tag);
                current.push_str("}}");
            } else if tag_trimmed == "/if" {
                depth -= 1;
                if depth == 0 {
                    // End of our if block
                    if in_else {
                        else_content = Some(current);
                    } else {
                        if_content = current;
                    }
                    return (if_content, else_content, pos);
                } else {
                    current.push_str("{{");
                    current.push_str(tag);
                    current.push_str("}}");
                }
            } else if tag_trimmed == "#else" && depth == 1 {
                // Switch to else branch (only at our depth)
                if_content = current;
                current = String::new();
                in_else = true;
            } else {
                // Other tag, preserve it
                current.push_str("{{");
                current.push_str(tag);
                current.push_str("}}");
            }
        } else {
            // Regular character
            current.push(template[pos..].chars().next().unwrap());
            pos += template[pos..].chars().next().unwrap().len_utf8();
        }
    }

    // Unclosed if block - return what we have
    if in_else {
        (if_content, Some(current), pos)
    } else {
        (current, None, pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_substitution() {
        let mut ctx = Template::new();
        ctx.set("name", "pihole");
        ctx.set("port", "53");

        let result = render_template("Service: {{name}} on port {{port}}", &ctx);
        assert_eq!(result, "Service: pihole on port 53");
    }

    #[test]
    fn test_missing_variable() {
        let ctx = Template::new();
        let result = render_template("Value: {{missing}}", &ctx);
        assert_eq!(result, "Value: ");
    }

    #[test]
    fn test_default_value() {
        let ctx = Template::new();
        let result = render_template("Value: {{missing|default_value}}", &ctx);
        assert_eq!(result, "Value: default_value");
    }

    #[test]
    fn test_default_not_used_when_set() {
        let mut ctx = Template::new();
        ctx.set("var", "actual");
        let result = render_template("Value: {{var|default}}", &ctx);
        assert_eq!(result, "Value: actual");
    }

    #[test]
    fn test_if_truthy() {
        let mut ctx = Template::new();
        ctx.set("static-ip", "192.168.1.100");

        let template = "{{#if static-ip}}IP: {{static-ip}}{{/if}}";
        let result = render_template(template, &ctx);
        assert_eq!(result, "IP: 192.168.1.100");
    }

    #[test]
    fn test_if_falsy() {
        let ctx = Template::new();

        let template = "{{#if static-ip}}IP: {{static-ip}}{{/if}}";
        let result = render_template(template, &ctx);
        assert_eq!(result, "");
    }

    #[test]
    fn test_if_negated() {
        let ctx = Template::new();

        let template = "{{#if !static-ip}}No static IP{{/if}}";
        let result = render_template(template, &ctx);
        assert_eq!(result, "No static IP");
    }

    #[test]
    fn test_if_else() {
        let mut ctx = Template::new();

        let template = "{{#if static-ip}}Static{{#else}}DHCP{{/if}}";

        let result = render_template(template, &ctx);
        assert_eq!(result, "DHCP");

        ctx.set("static-ip", "192.168.1.100");
        let result = render_template(template, &ctx);
        assert_eq!(result, "Static");
    }

    #[test]
    fn test_if_equality() {
        let mut ctx = Template::new();
        ctx.set("os", "linux");

        let template = "{{#if os=linux}}Linux{{/if}}{{#if os=windows}}Windows{{/if}}";
        let result = render_template(template, &ctx);
        assert_eq!(result, "Linux");
    }

    #[test]
    fn test_nested_if() {
        let mut ctx = Template::new();
        ctx.set("static-ip", "192.168.1.100");
        ctx.set("os", "linux");

        let template = "{{#if static-ip}}{{#if os=linux}}Linux static{{/if}}{{/if}}";
        let result = render_template(template, &ctx);
        assert_eq!(result, "Linux static");
    }

    #[test]
    fn test_multiline_template() {
        let mut ctx = Template::new();
        ctx.set("name", "pihole");
        ctx.set("static-ip", "192.168.1.100");

        let template = r#"# Service: {{name}}

{{#if static-ip}}
DNS: {{static-ip}}
{{#else}}
Run: ping {{name}}
{{/if}}
"#;

        let result = render_template(template, &ctx);
        assert!(result.contains("DNS: 192.168.1.100"));
        assert!(!result.contains("Run: ping"));
    }

    #[test]
    fn test_is_truthy() {
        let mut ctx = Template::new();

        assert!(!ctx.is_truthy("missing"));

        ctx.set("empty", "");
        assert!(!ctx.is_truthy("empty"));

        ctx.set("value", "something");
        assert!(ctx.is_truthy("value"));
    }

    #[test]
    fn test_set_opt() {
        let mut ctx = Template::new();

        ctx.set_opt("none", None::<String>);
        assert!(!ctx.is_truthy("none"));

        ctx.set_opt("some", Some("value"));
        assert!(ctx.is_truthy("some"));
    }
}
