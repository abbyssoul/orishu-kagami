//! Interactive REPL over `VariablesSystem`.
//!
//! Run with `cargo run -p orishu-variables --example repl`. Lines of the
//! form `namespace.name = expr` define (or redefine) a variable; any other
//! line is evaluated as an ad-hoc expression. `quit`/`exit` ends the session.
use std::sync::{Arc, Mutex};

use orishu_variables::{CompiledExpression, FQName, VariableOptions, VariablesSystem};
use reedline::{
    ColumnarMenu, Completer, CompletionResult, DefaultCompleter, DefaultPrompt,
    DefaultPromptSegment, Emacs, KeyCode, KeyModifiers, MenuBuilder, Reedline, ReedlineEvent,
    ReedlineMenu, Signal, default_emacs_keybindings,
};

/// `DefaultCompleter` behind a shared handle so new vars/namespaces can be
/// added to its word list after it has been handed off to `Reedline`.
#[derive(Clone)]
struct SharedCompleter(Arc<Mutex<DefaultCompleter>>);

impl SharedCompleter {
    fn new(completer: DefaultCompleter) -> Self {
        Self(Arc::new(Mutex::new(completer)))
    }

    fn insert(&self, words: Vec<String>) {
        self.0.lock().unwrap().insert(words);
    }
}

impl Completer for SharedCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> CompletionResult {
        self.0.lock().unwrap().complete(line, pos)
    }
}

fn repl(vars: &mut VariablesSystem, line: &str) -> bool {
    match line.split_once('=') {
        Some((qualified_name, expr_source)) if !qualified_name.trim().is_empty() => {
            let qualified_name = qualified_name.trim();
            let expr_source = expr_source.trim();
            let fqname = match FQName::parse(qualified_name) {
                Ok(fqname) => fqname,
                Err(err) => {
                    println!("error parsing name: {err}");
                    return true;
                }
            };

            let expr_res = CompiledExpression::parse(expr_source);
            if let Err(err) = expr_res {
                println!("error parsing expression: {err}");
                return true;
            }

            let result = match vars.lookup(&fqname) {
                Some(id) => vars.set(id, expr_res.unwrap()).map(|()| id),
                None => vars.define(
                    fqname.namespace(),
                    fqname.name().clone(),
                    expr_res.unwrap(),
                    VariableOptions::default(),
                ),
            };

            match result {
                Ok(id) => match vars.value(id) {
                    Ok(value) => {
                        println!("{qualified_name} = {value}");
                        true
                    }
                    Err(err) => {
                        println!("no such var: {err}");
                        true
                    }
                },
                Err(err) => {
                    println!("error: {err}");
                    true
                }
            }
        }
        _ => match vars.eval(line) {
            Ok(value) => {
                println!("{value}");
                true
            }
            Err(err) => {
                println!("eval error: {err}");
                true
            }
        },
    }
}

fn main() {
    let mut vars = VariablesSystem::default();

    let prompt = DefaultPrompt::new(
        DefaultPromptSegment::Basic(">".into()),
        DefaultPromptSegment::CurrentDateTime,
    );

    let completer = SharedCompleter::new(DefaultCompleter::default().set_min_word_len(1));
    // Use the interactive menu to select options from the completer
    let completion_menu = Box::new(ColumnarMenu::default().with_name("completion_menu"));
    // Set up the required keybindings
    let mut keybindings = default_emacs_keybindings();
    keybindings.add_binding(
        KeyModifiers::NONE,
        KeyCode::Tab,
        ReedlineEvent::UntilFound(vec![
            ReedlineEvent::Menu("completion_menu".to_string()),
            ReedlineEvent::MenuNext,
        ]),
    );

    let b = Box::new(completer);
    let edit_mode = Box::new(Emacs::new(keybindings));
    // let mut line_editor = Reedline::create();
    let mut line_editor = Reedline::create()
        .with_completer(b.clone())
        .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
        .with_edit_mode(edit_mode);

    loop {
        let sig = line_editor.read_line(&prompt);
        match sig {
            Ok(Signal::Success(buffer)) => {
                repl(&mut vars, &buffer);

                b.as_ref().insert(
                    vars.variables()
                        .map(|var| format!("{}", var.name))
                        .collect(),
                );
            }
            Ok(Signal::CtrlD) | Ok(Signal::CtrlC) => {
                println!("\ndone");
                break;
            }
            x => {
                println!("Event: {:?}", x);
            }
        }
    }
}
