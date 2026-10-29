use crate::{Analysis, DomResolver, JsOutput, ScriptInput};

pub(crate) fn rewrite(
    _scripts: &[ScriptInput],
    _analysis: &Analysis,
    _resolver: &dyn DomResolver,
) -> JsOutput {
    unimplemented!("h2q_js worker")
}
