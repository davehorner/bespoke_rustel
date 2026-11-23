//! Read-only metadata for shared definitions. Never execute score code to inspect it.
use super::*;
use oxc_ast::ast::IdentifierReference;

#[derive(Clone, Debug)]
pub struct Definition {
    pub name: String,
    pub value: Option<f64>,
    pub dependencies: Vec<String>,
}

#[derive(Default)]
struct References(Vec<String>);
impl<'a> Visit<'a> for References {
    fn visit_identifier_reference(&mut self, id: &IdentifierReference<'a>) {
        if !self.0.iter().any(|name| name == id.name.as_str()) {
            self.0.push(id.name.to_string());
        }
    }
}

fn number(expression: &Expression<'_>, known: &[Definition]) -> Option<f64> {
    let value = match expression {
        Expression::NumericLiteral(n) => n.value,
        Expression::Identifier(id) => known.iter().find(|d| d.name == id.name.as_str())?.value?,
        Expression::ParenthesizedExpression(p) => return number(&p.expression, known),
        Expression::UnaryExpression(u) => {
            let value = number(&u.argument, known)?;
            match u.operator.as_str() {
                "+" => value,
                "-" => -value,
                _ => return None,
            }
        }
        Expression::BinaryExpression(b) => {
            let a = number(&b.left, known)?;
            let b_value = number(&b.right, known)?;
            match b.operator.as_str() {
                "+" => a + b_value,
                "-" => a - b_value,
                "*" => a * b_value,
                "/" => a / b_value,
                _ => return None,
            }
        }
        _ => return None,
    };
    value.is_finite().then_some(value)
}

pub fn definitions(source: &str) -> Vec<Definition> {
    if check_nesting(source).is_err() {
        return Vec::new();
    }
    on_parse_stack(|| {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();
        if parsed.panicked || !parsed.diagnostics.is_empty() {
            return Vec::new();
        }
        let mut result = Vec::new();
        for statement in &parsed.program.body {
            if let Statement::VariableDeclaration(decl) = statement {
                if decl.kind != VariableDeclarationKind::Const {
                    continue;
                }
                for binding in &decl.declarations {
                    if let (BindingPattern::BindingIdentifier(id), Some(init)) =
                        (&binding.id, &binding.init)
                    {
                        let mut references = References::default();
                        references.visit_expression(init);
                        result.push(Definition {
                            name: id.name.to_string(),
                            value: number(init, &result),
                            dependencies: references.0,
                        });
                    }
                }
            }
        }
        result
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scalar_expressions_and_dependencies_are_read_without_running_calls() {
        let d = definitions(
            "const bpm=120, half=bpm/2; const harmony=chord('Am').dict('ireal'); const rhythm=harmony.s('piano'); const unsafeValue=sideEffect()",
        );
        assert_eq!(d[1].value, Some(60.0));
        assert!(d[3].dependencies.contains(&"harmony".to_owned()));
        assert_eq!(d[4].value, None);
    }
}
