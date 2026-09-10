//! Resource bounds, exercised through the public API.
//!
//! These are S-VARIABLES slice 1's declared bounds: source length, parse
//! depth, node count, variable count, dependency depth, and evaluation work.
//! Every dimension is checked twice — exactly at the bound it is accepted,
//! one past it, refused — because a bound that only ever fires late is
//! indistinguishable from no bound at all, and one that fires early quietly
//! refuses legitimate work.
//!
//! Everything here drives the same entry points a caller uses. Nothing reaches
//! into the parser or the evaluator, because "an MCP client cannot exhaust
//! this process" is a claim about the public surface.

use std::collections::BTreeMap;

use orishu_variables::{
    CompiledExpression, ExprParsingError, LimitError, LimitKind, Limits, Name, Namespace,
    VariableId, VariableOptions, VariablesError, VariablesSystem, rewrite_symbols_bounded,
};

fn ns(value: &str) -> Namespace {
    Namespace::new(value)
}

/// The bound a parse refused, or `None` if it failed for another reason.
fn parse_refusal(result: Result<CompiledExpression, ExprParsingError>) -> Option<LimitError> {
    result.err().and_then(|error| error.limit_error())
}

/// The bound an operation on a system refused.
fn refusal<T: std::fmt::Debug>(result: Result<T, VariablesError>) -> LimitError {
    match result {
        Err(VariablesError::Limit(error)) => error,
        Err(VariablesError::Parsing(error)) => error
            .limit_error()
            .unwrap_or_else(|| panic!("a syntax error, not a bound: {error}")),
        other => panic!("expected a bound refusal, got {other:?}"),
    }
}

/// Define `source` as `chain.v{index}`.
fn define(
    vars: &mut VariablesSystem,
    index: usize,
    source: &str,
) -> Result<VariableId, VariablesError> {
    vars.define(
        &ns("chain"),
        format!("v{index}"),
        CompiledExpression::parse_bounded(source, vars.limits())?,
        VariableOptions::default(),
    )
}

/// `chain.v0 = 0`, `chain.v1 = chain.v0 + 1`, ..., returning the tail.
///
/// `links` is the number of *edges*, so the resolution walks `links + 1`
/// frames — the unit `max_dependency_depth` counts.
fn build_chain(vars: &mut VariablesSystem, links: usize) -> Result<VariableId, VariablesError> {
    let mut tail = define(vars, 0, "0")?;
    for i in 1..=links {
        tail = define(vars, i, &format!("chain.v{} + 1", i - 1))?;
    }
    Ok(tail)
}

// -- expression source bytes ------------------------------------------------

#[test]
fn expression_source_is_accepted_at_the_byte_bound_and_refused_one_over() {
    let limits = Limits {
        max_expression_bytes: 16,
        ..Limits::DEFAULT
    };
    let at = "1234567890123456";
    assert_eq!(at.len(), 16);
    assert!(CompiledExpression::parse_bounded(at, &limits).is_ok());

    let over = format!("{at}7");
    assert_eq!(
        parse_refusal(CompiledExpression::parse_bounded(&over, &limits)),
        Some(LimitError {
            limit: LimitKind::ExpressionBytes,
            found: 17,
            allowed: 16,
        })
    );
}

#[test]
fn an_oversized_source_is_refused_with_a_span_at_the_first_byte_over_budget() {
    // The span is the useful part of the refusal: it says where the caller's
    // budget ran out, not merely that it did.
    let limits = Limits {
        max_expression_bytes: 3,
        ..Limits::DEFAULT
    };
    let error = CompiledExpression::parse_bounded("1 + 2 + 3", &limits).unwrap_err();
    assert_eq!(error.span.start, 3);
    assert_eq!(error.span.end, 9);
}

#[test]
fn the_ad_hoc_eval_path_carries_the_systems_byte_bound() {
    // `eval` parses too, and a convenience entry point that skipped the bound
    // would be the whole hardening undone.
    let vars = VariablesSystem::with_limits(Limits {
        max_expression_bytes: 5,
        ..Limits::DEFAULT
    });
    assert_eq!(vars.eval("1 + 1").unwrap().magnitude(), 2.0);
    assert_eq!(
        refusal(vars.eval("1 + 12")).limit,
        LimitKind::ExpressionBytes
    );
}

// -- parse depth ------------------------------------------------------------

#[test]
fn nesting_is_accepted_at_the_depth_bound_and_refused_one_over() {
    let limits = Limits {
        max_parse_depth: 12,
        ..Limits::DEFAULT
    };
    // The outermost parse counts as one level, so eleven parentheses reach
    // exactly twelve.
    let nested = |count: usize| format!("{}1{}", "(".repeat(count), ")".repeat(count));
    assert!(CompiledExpression::parse_bounded(&nested(11), &limits).is_ok());
    assert_eq!(
        parse_refusal(CompiledExpression::parse_bounded(&nested(12), &limits)),
        Some(LimitError {
            limit: LimitKind::ParseDepth,
            found: 13,
            allowed: 12,
        })
    );
}

#[test]
fn a_deeply_nested_source_is_refused_rather_than_exhausting_the_stack() {
    // Under `Limits::DEFAULT`, and well inside the default byte budget, so
    // depth is unambiguously what refuses it. Without the bound this is a
    // stack overflow — an abort, not an error a caller can report.
    let nested = format!("{}1{}", "(".repeat(2_000), ")".repeat(2_000));
    assert!(nested.len() < Limits::DEFAULT.max_expression_bytes);
    assert_eq!(
        parse_refusal(CompiledExpression::parse(&nested)).map(|error| error.limit),
        Some(LimitKind::ParseDepth)
    );
}

#[test]
fn a_wide_flat_expression_is_bounded_by_the_tree_it_builds() {
    // `1+1+…` folds left without the parser recursing, so only measuring the
    // *tree* stops it. Everything that later walks that tree — symbol
    // collection, `is_const`, evaluation — recurses once per level.
    let limits = Limits {
        max_parse_depth: 10,
        ..Limits::DEFAULT
    };
    let chain = |terms: usize| vec!["1"; terms].join("+");
    assert!(CompiledExpression::parse_bounded(&chain(10), &limits).is_ok());
    assert_eq!(
        parse_refusal(CompiledExpression::parse_bounded(&chain(11), &limits))
            .map(|error| error.limit),
        Some(LimitKind::ParseDepth)
    );
}

#[test]
fn walking_a_maximally_deep_expression_stays_within_the_stack() {
    // The reason the tree is bounded at all: these three walks are recursive,
    // and a source accepted by the parser must be safe for all of them.
    let terms = Limits::DEFAULT.max_parse_depth;
    let source = (0..terms)
        .map(|i| format!("a{i}"))
        .collect::<Vec<_>>()
        .join("+");
    let expression = CompiledExpression::parse(&source).expect("exactly at the depth bound");
    assert_eq!(expression.depth(), terms);
    assert_eq!(expression.variables().len(), terms);
    assert!(!expression.is_const());
}

// -- AST node count ---------------------------------------------------------

#[test]
fn node_count_is_accepted_at_the_bound_and_refused_one_over() {
    // Balanced parentheses, so the depth bound cannot be what fires: this is
    // the width of the tree, not its height.
    let limits = Limits {
        max_expression_nodes: 7,
        ..Limits::DEFAULT
    };
    let at = CompiledExpression::parse_bounded("(1+1)+(1+1)", &limits).expect("seven nodes");
    assert_eq!(at.nodes(), 7);

    assert_eq!(
        parse_refusal(CompiledExpression::parse_bounded(
            "(1+1)+((1+1)+1)",
            &limits
        )),
        Some(LimitError {
            limit: LimitKind::ExpressionNodes,
            found: 8,
            allowed: 7,
        })
    );
}

#[test]
fn a_system_refuses_an_expression_parsed_under_looser_bounds() {
    // A `CompiledExpression` always passed *some* bound, but not necessarily
    // this system's. Without the re-check, choosing where to parse would
    // choose which bounds apply.
    let loose = CompiledExpression::parse("1+1+1+1+1+1").expect("within the defaults");
    let mut vars = VariablesSystem::with_limits(Limits {
        max_expression_nodes: 5,
        ..Limits::DEFAULT
    });
    let refused = vars.define(
        &ns("globals"),
        "wide",
        loose.clone(),
        VariableOptions::default(),
    );
    assert_eq!(refusal(refused).limit, LimitKind::ExpressionNodes);

    // And the same on the redefinition path.
    let narrow = CompiledExpression::parse("1+1").expect("small");
    let id = vars
        .define(&ns("globals"), "wide", narrow, VariableOptions::default())
        .expect("three nodes");
    assert_eq!(
        vars.set(id, loose).map_err(|e| refusal::<()>(Err(e)).limit),
        Err(LimitKind::ExpressionNodes)
    );
}

// -- variable count ---------------------------------------------------------

#[test]
fn variables_are_accepted_up_to_the_count_bound_and_refused_one_over() {
    let mut vars = VariablesSystem::with_limits(Limits {
        max_variables: 8,
        ..Limits::DEFAULT
    });
    for i in 0..8 {
        define(&mut vars, i, "1").expect("within the bound");
    }
    assert_eq!(vars.variables().count(), 8);

    assert_eq!(
        refusal(define(&mut vars, 8, "1")),
        LimitError {
            limit: LimitKind::Variables,
            found: 9,
            allowed: 8,
        }
    );
}

#[test]
fn a_refused_definition_changes_nothing() {
    let mut vars = VariablesSystem::with_limits(Limits {
        max_variables: 2,
        ..Limits::DEFAULT
    });
    define(&mut vars, 0, "1").unwrap();
    let second = define(&mut vars, 1, "chain.v0 + 1").unwrap();

    assert!(define(&mut vars, 2, "chain.v1 + 1").is_err());

    // Not one of: an appended variable, an index entry, a consumed handle.
    assert_eq!(vars.variables().count(), 2);
    assert_eq!(vars.namespaces().collect::<Vec<_>>(), vec![ns("chain")]);
    assert_eq!(
        vars.lookup_in(&ns("chain"), &Name::new("v2").unwrap()),
        None
    );
    assert!(vars.resolve_symbol("chain.v2").is_err());
    assert_eq!(vars.value(second).unwrap().magnitude(), 2.0);
    assert_eq!(vars.variables_in(&ns("chain")).count(), 2);
}

#[test]
fn a_refused_redefinition_leaves_the_previous_expression_in_place() {
    let mut vars = VariablesSystem::with_limits(Limits {
        max_expression_nodes: 3,
        ..Limits::DEFAULT
    });
    let id = define(&mut vars, 0, "2+2").unwrap();
    let oversized = CompiledExpression::parse("1+1+1+1").expect("within the defaults");

    assert_eq!(
        refusal(vars.set(id, oversized)).limit,
        LimitKind::ExpressionNodes
    );

    // The value, and therefore the expression behind it, is untouched.
    assert_eq!(vars.value(id).unwrap().magnitude(), 4.0);
    assert_eq!(vars.variables().count(), 1);

    // And the system still accepts a legal edit afterwards.
    vars.set(id, CompiledExpression::parse("3+3").unwrap())
        .expect("three nodes");
    assert_eq!(vars.value(id).unwrap().magnitude(), 6.0);
}

// -- dependency depth -------------------------------------------------------

#[test]
fn a_chain_resolves_at_the_depth_bound_and_is_refused_one_link_deeper() {
    let limits = Limits {
        max_dependency_depth: 16,
        ..Limits::DEFAULT
    };
    let mut vars = VariablesSystem::with_limits(limits);
    // Sixteen frames: the root plus fifteen links.
    let tail = build_chain(&mut vars, 15).expect("within the bound");
    assert_eq!(vars.value(tail).unwrap().magnitude(), 15.0);

    let deeper = define(&mut vars, 16, "chain.v15 + 1").expect("defining is not resolving");
    assert_eq!(
        refusal(vars.value(deeper)),
        LimitError {
            limit: LimitKind::DependencyDepth,
            found: 17,
            allowed: 16,
        }
    );
}

#[test]
fn the_ad_hoc_eval_path_carries_the_same_dependency_bound() {
    let mut vars = VariablesSystem::with_limits(Limits {
        max_dependency_depth: 4,
        ..Limits::DEFAULT
    });
    build_chain(&mut vars, 4).expect("five variables");
    assert_eq!(vars.eval("chain.v3 + 1").unwrap().magnitude(), 4.0);
    assert_eq!(
        refusal(vars.eval("chain.v4 + 1")).limit,
        LimitKind::DependencyDepth
    );
}

#[test]
fn depth_does_not_depend_on_the_order_dependencies_are_written_in() {
    // `root -> b -> a` is three levels deep whichever way round the root
    // names its two dependencies. Reaching `a` directly first would memoize
    // it, and a memo that forgot the chain beneath it would let the same
    // graph pass or fail depending on the spelling.
    let outcomes: Vec<bool> = ["g.a + g.b", "g.b + g.a"]
        .into_iter()
        .map(|source| {
            let mut vars = VariablesSystem::with_limits(Limits {
                max_dependency_depth: 2,
                ..Limits::DEFAULT
            });
            let scope = ns("g");
            for (name, expression) in [("a", "1"), ("b", "g.a")] {
                vars.define(
                    &scope,
                    name,
                    CompiledExpression::parse(expression).unwrap(),
                    VariableOptions::default(),
                )
                .unwrap();
            }
            let root = vars
                .define(
                    &scope,
                    "root",
                    CompiledExpression::parse(source).unwrap(),
                    VariableOptions::default(),
                )
                .unwrap();
            matches!(
                vars.value(root),
                Err(VariablesError::Limit(LimitError {
                    limit: LimitKind::DependencyDepth,
                    ..
                }))
            )
        })
        .collect();
    assert_eq!(outcomes, vec![true, true]);
}

#[test]
fn a_shared_dependency_is_admitted_at_its_true_depth_either_way_round() {
    // The same two graphs one level further up: three is exactly the depth,
    // so both spellings must now be accepted. Together with the case above
    // this pins the bound to the graph rather than to the walk — it is not
    // enough for the two orders to agree if they agree on the wrong number.
    for source in ["g.a + g.b", "g.b + g.a"] {
        let mut vars = VariablesSystem::with_limits(Limits {
            max_dependency_depth: 3,
            ..Limits::DEFAULT
        });
        let scope = ns("g");
        for (name, expression) in [("a", "2"), ("b", "g.a")] {
            vars.define(
                &scope,
                name,
                CompiledExpression::parse(expression).unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        }
        let root = vars
            .define(
                &scope,
                "root",
                CompiledExpression::parse(source).unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        assert_eq!(
            vars.value(root).expect(source).magnitude(),
            4.0,
            "{source} should resolve at exactly the bound"
        );
    }
}

#[test]
fn a_diamond_is_charged_the_depth_of_its_longest_side() {
    // `root` reaches `shallow` directly and `deep` through a chain, so the
    // graph is four deep however the memo happens to fill. Reaching the
    // shared leaf by its short path first must not discount the long one.
    let build = |limit: usize| {
        let mut vars = VariablesSystem::with_limits(Limits {
            max_dependency_depth: limit,
            ..Limits::DEFAULT
        });
        let scope = ns("g");
        for (name, expression) in [
            ("leaf", "1"),
            ("mid", "g.leaf + 1"),
            ("deep", "g.mid + 1"),
            ("root", "g.leaf + g.deep"),
        ] {
            vars.define(
                &scope,
                name,
                CompiledExpression::parse(expression).unwrap(),
                VariableOptions::default(),
            )
            .unwrap();
        }
        let root = vars.resolve_symbol("g.root").unwrap();
        vars.value(root)
    };
    assert_eq!(build(4).expect("exactly four deep").magnitude(), 4.0);
    assert_eq!(
        refusal(build(3)),
        LimitError {
            limit: LimitKind::DependencyDepth,
            found: 4,
            allowed: 3,
        }
    );
}

#[test]
fn a_long_chain_is_refused_by_depth_rather_than_by_running_out_of_stack() {
    // Under `Limits::DEFAULT`, whose dependency bound is far below what the
    // iterative resolver could actually walk. The refusal is the declared
    // bound, not an exhausted resource.
    let mut vars = VariablesSystem::default();
    let tail = build_chain(&mut vars, Limits::DEFAULT.max_dependency_depth).expect("defined");
    assert_eq!(refusal(vars.value(tail)).limit, LimitKind::DependencyDepth);
}

// -- evaluation work --------------------------------------------------------

#[test]
fn evaluation_work_is_accepted_at_the_bound_and_refused_one_unit_short() {
    // One variable, five nodes: `(1+1)+1` is three literals and two adds. A
    // resolution walks it twice — once to collect its references, once to
    // compute it — so the whole evaluation costs ten.
    let mut vars = VariablesSystem::with_limits(Limits {
        max_evaluation_work: 10,
        ..Limits::DEFAULT
    });
    let id = define(&mut vars, 0, "(1+1)+1").unwrap();
    assert_eq!(vars.value(id).unwrap().magnitude(), 3.0);

    let mut tighter = VariablesSystem::with_limits(Limits {
        max_evaluation_work: 9,
        ..Limits::DEFAULT
    });
    let id = define(&mut tighter, 0, "(1+1)+1").unwrap();
    assert_eq!(
        refusal(tighter.value(id)),
        LimitError {
            limit: LimitKind::EvaluationWork,
            found: 10,
            allowed: 9,
        }
    );
}

#[test]
fn a_wide_shared_graph_is_refused_by_work_rather_than_by_depth_or_count() {
    // The case neither the variable count nor the dependency depth catches: a
    // graph 30 levels deep and 30 variables wide, where every level references
    // the one below it twice. Memoization means each variable is evaluated
    // once, so this is honest work rather than exponential blow-up — and it is
    // still more work than the budget allows.
    let limits = Limits {
        max_variables: 64,
        max_dependency_depth: 64,
        max_evaluation_work: 100,
        ..Limits::DEFAULT
    };
    let mut vars = VariablesSystem::with_limits(limits);
    define(&mut vars, 0, "1").unwrap();
    let mut tail = None;
    for i in 1..30 {
        let source = format!("(chain.v{prev} + chain.v{prev}) / 2", prev = i - 1);
        tail = Some(define(&mut vars, i, &source).unwrap());
    }
    let tail = tail.unwrap();

    // Comfortably inside the other two bounds, so only work can refuse it.
    assert!(vars.variables().count() < limits.max_variables);
    let error = refusal(vars.value(tail));
    assert_eq!(error.limit, LimitKind::EvaluationWork);
    assert_eq!(error.allowed, 100);

    // Raise only the budget and the same graph resolves, which is what makes
    // this a work bound rather than a disguised depth or count one.
    let mut wider = VariablesSystem::with_limits(Limits {
        max_evaluation_work: 10_000,
        ..limits
    });
    define(&mut wider, 0, "1").unwrap();
    let mut tail = None;
    for i in 1..30 {
        let source = format!("(chain.v{prev} + chain.v{prev}) / 2", prev = i - 1);
        tail = Some(define(&mut wider, i, &source).unwrap());
    }
    assert_eq!(wider.value(tail.unwrap()).unwrap().magnitude(), 1.0);
}

#[test]
fn each_top_level_evaluation_gets_its_own_budget() {
    // Otherwise a long-lived system would slowly refuse everything, and the
    // bound would be on the process rather than on the call.
    let mut vars = VariablesSystem::with_limits(Limits {
        max_evaluation_work: 6,
        ..Limits::DEFAULT
    });
    let id = define(&mut vars, 0, "1+1").unwrap();
    for _ in 0..100 {
        assert_eq!(vars.value(id).unwrap().magnitude(), 2.0);
    }
}

// -- rewriting --------------------------------------------------------------

#[test]
fn rewriting_is_bounded_at_both_ends() {
    let limits = Limits {
        max_expression_bytes: 8,
        ..Limits::DEFAULT
    };
    let renames = BTreeMap::from([("a".to_owned(), "bbbb".to_owned())]);

    // Input over the bound, refused before it is scanned.
    assert_eq!(
        rewrite_symbols_bounded("a + a + a", &renames, &limits),
        Err(LimitError {
            limit: LimitKind::ExpressionBytes,
            found: 9,
            allowed: 8,
        })
    );

    // Input inside the bound, output over it — a property of the caller's
    // rename map, which nothing about the source could have revealed.
    // `bbbb + ` is seven bytes, and the second replacement would take it to
    // eleven, so the refusal names the length it stopped at rather than the
    // length the whole rewrite would have reached.
    assert_eq!(
        rewrite_symbols_bounded("a + a", &renames, &limits),
        Err(LimitError {
            limit: LimitKind::ExpressionBytes,
            found: 11,
            allowed: 8,
        })
    );

    // Exactly at the bound.
    assert_eq!(
        rewrite_symbols_bounded("a + 1", &renames, &limits),
        Ok("bbbb + 1".to_owned())
    );
}

// -- the semantics the bounds must not have changed -------------------------

#[test]
fn bounding_did_not_change_dimension_cycle_or_quantity_semantics() {
    let mut vars = VariablesSystem::default();

    // Dimensions still derive from the arithmetic, and mismatches are still
    // dimension errors rather than resource ones.
    assert_eq!(
        vars.eval("2.7 g / cm^3").unwrap().dimension(),
        orishu_variables::Dimension::DENSITY
    );
    assert!(matches!(
        vars.eval("1 kg + 1 m"),
        Err(VariablesError::Eval(
            orishu_variables::ExprEvalError::DimensionMismatch { .. }
        ))
    ));
    assert!(matches!(
        vars.eval("1 / 0"),
        Err(VariablesError::Eval(
            orishu_variables::ExprEvalError::DivisionByZero
        ))
    ));

    // A cycle is still a cycle, not a depth or work refusal: it is detected on
    // the edge back into an in-progress frame, before either bound can grow.
    let id = vars
        .define(
            &ns("globals"),
            "a",
            CompiledExpression::parse("globals.a + 1").unwrap(),
            VariableOptions::default(),
        )
        .unwrap();
    assert!(matches!(
        vars.value(id),
        Err(VariablesError::Eval(orishu_variables::ExprEvalError::Cycle))
    ));

    // A root name that would shadow a unit is still refused as such.
    assert!(matches!(
        vars.define(
            &ns(""),
            "m",
            CompiledExpression::parse("1").unwrap(),
            VariableOptions::default()
        ),
        Err(VariablesError::ShadowsUnit { .. })
    ));
}
