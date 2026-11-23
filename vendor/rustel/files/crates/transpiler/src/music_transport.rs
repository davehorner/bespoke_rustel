//! Source-preserving transport edits using parser spans, never string matches
//! inside comments, mini notation, or unrelated identifier names.
use super::*;

struct Calls {
    allowed: HashSet<usize>,
    ranges: Vec<std::ops::Range<usize>>,
}
impl<'a> Visit<'a> for Calls {
    fn visit_call_expression(&mut self, call: &CallExpression<'a>) {
        if self.allowed.contains(&(call.callee.span().start as usize)) {
            self.ranges
                .push(call.span.start as usize..call.span.end as usize);
        }
        walk::walk_call_expression(self, call);
    }
}

pub fn time_signature(source: &str) -> (u32, u32) {
    on_parse_stack(|| {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();
        let mut result = (4, 4);
        for statement in &parsed.program.body {
            if let Statement::VariableDeclaration(decl) = statement {
                for binding in &decl.declarations {
                    if let (
                        BindingPattern::BindingIdentifier(id),
                        Some(Expression::NumericLiteral(value)),
                    ) = (&binding.id, &binding.init)
                    {
                        if value.value >= 1.0 && value.value <= 64.0 && value.value.fract() == 0.0 {
                            if id.name == "timeSignatureTop" {
                                result.0 = value.value as u32;
                            }
                            if id.name == "timeSignatureBottom"
                                && (value.value as u32).is_power_of_two()
                            {
                                result.1 = value.value as u32;
                            }
                        }
                    }
                }
            }
        }
        result
    })
    .unwrap_or((4, 4))
}

pub fn rewrite(source: &str, bpm: f64, top: u32, bottom: u32) -> Result<String, String> {
    if !bpm.is_finite()
        || !(1.0..=1000.0).contains(&bpm)
        || !(1..=64).contains(&top)
        || !bottom.is_power_of_two()
        || bottom > 64
    {
        return Err("invalid transport settings".into());
    }
    if check_nesting(source).is_err() {
        return Err("source nesting limit".into());
    }
    on_parse_stack(|| {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::mjs()).parse();
        if parsed.panicked || !parsed.diagnostics.is_empty() {
            return Err("cannot update invalid source".into());
        }
        let settings = [
            ("bpm", bpm.to_string()),
            ("timeSignatureTop", top.to_string()),
            ("timeSignatureBottom", bottom.to_string()),
        ];
        let mut found = [false; 3];
        let mut anchor = 0;
        let mut edits = Vec::<(usize, usize, String)>::new();
        for statement in &parsed.program.body {
            if let Statement::VariableDeclaration(decl) = statement {
                for binding in &decl.declarations {
                    if let BindingPattern::BindingIdentifier(id) = &binding.id {
                        if let Some(index) = settings.iter().position(|(name, _)| id.name == *name)
                        {
                            found[index] = true;
                            anchor = anchor.max(decl.span.end as usize);
                            if let Some(init) = &binding.init {
                                let span = init.span();
                                edits.push((
                                    span.start as usize,
                                    span.end as usize,
                                    settings[index].1.clone(),
                                ));
                            } else {
                                edits.push((
                                    id.span.end as usize,
                                    id.span.end as usize,
                                    format!(" = {}", settings[index].1),
                                ));
                            }
                        }
                    }
                }
            }
        }
        let mut calls = Calls {
            allowed: running_calls(source, &["setcpm", "setcps"])
                .into_iter()
                .map(|(_, r)| r.start)
                .collect(),
            ranges: Vec::new(),
        };
        calls.visit_program(&parsed.program);
        let existing = calls
            .ranges
            .iter()
            .find(|range| {
                range.start >= anchor
                    && !edits
                        .iter()
                        .any(|(a, b, _)| *a < range.end && range.start < *b)
            })
            .cloned();
        for range in calls.ranges {
            if edits
                .iter()
                .any(|(a, b, _)| *a < range.end && range.start < *b)
            {
                continue;
            }
            if existing.as_ref() == Some(&range) {
                edits.push((
                    range.start,
                    range.end,
                    "setcpm(bpm * timeSignatureBottom / (4 * timeSignatureTop))".into(),
                ));
                continue;
            }
            let statement = parsed
                .program
                .body
                .iter()
                .find_map(|statement| match statement {
                    Statement::ExpressionStatement(e)
                        if e.expression.span().start as usize == range.start
                            && e.expression.span().end as usize == range.end =>
                    {
                        Some(e.span)
                    }
                    _ => None,
                });
            if let Some(span) = statement {
                edits.push((span.start as usize, span.end as usize, String::new()));
            } else {
                edits.push((range.start, range.end, "void 0".into()));
            }
        }
        let eol = if source.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let mut prefix = String::new();
        for (index, (name, value)) in settings.iter().enumerate() {
            if !found[index] {
                prefix.push_str(&format!("const {name} = {value};{eol}"));
            }
        }
        let call =
            format!(";{eol}setcpm(bpm * timeSignatureBottom / (4 * timeSignatureTop));{eol}");
        if existing.is_none() {
            if anchor == 0 {
                prefix.push_str(&call);
            } else {
                edits.push((anchor, anchor, call));
            }
        }
        if !prefix.is_empty() {
            edits.push((0, 0, prefix));
        }
        edits.sort_by_key(|(from, to, _)| (*from, *to));
        let mut result = source.to_owned();
        for (from, to, text) in edits.into_iter().rev() {
            result.replace_range(from..to, &text);
        }
        Ok(result)
    })
    .ok_or_else(|| "cannot start source parser".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transport_rewrite_preserves_multiline_patterns_comments_and_other_bindings() {
        let source = "// setcpm(9)\r\nsetcpm(124/4)\r\nconst bpm = 124, keep = 7;\r\nconst bass = note(\"c3 \\\r\n e3\");\r\n$: bass\r\n";
        let updated = rewrite(source, 135.0, 6, 8).unwrap();
        assert!(updated.contains("const bpm = 135, keep = 7;"));
        assert!(updated.contains("// setcpm(9)"));
        assert!(updated.contains("note(\"c3 \\\r\n e3\")"));
        assert_eq!(time_signature(&updated), (6, 8));
        assert_eq!(running_calls(&updated, &["setcpm", "setcps"]).len(), 1);
        assert!(updated.ends_with("$: bass\r\n"));
        assert_eq!(rewrite(&updated, 135.0, 6, 8).unwrap(), updated);
    }
    #[test]
    fn transport_can_be_added_without_stealing_the_last_pattern_expression() {
        let updated = rewrite("note(\"c3 e3\")", 90.0, 3, 4).unwrap();
        assert!(updated.ends_with("note(\"c3 e3\")"));
        assert_eq!(time_signature(&updated), (3, 4));
        assert!(
            transpile(&updated, &TranspileOptions::default())
                .diagnostics
                .is_empty()
        );
    }
}
