use crate::{app::Route, components::execution_list_page::ExecutionQuery, grpc::ffqn::FunctionFqn};
use yew::prelude::*;
use yew_router::prelude::Link;
use yewprint::Icon;

#[derive(Properties, PartialEq)]
pub struct FfqnWithLinksProps {
    pub ffqn: FunctionFqn,
    #[prop_or_default]
    pub fully_qualified: bool,
    #[prop_or_default]
    pub hide_submit: bool,
    #[prop_or_default]
    pub hide_find: bool,
}
#[function_component(FfqnWithLinks)]
pub fn ffqn_with_links(
    FfqnWithLinksProps {
        ffqn,
        fully_qualified,
        hide_submit,
        hide_find,
    }: &FfqnWithLinksProps,
) -> Html {
    let ext = ffqn.ifc_fqn.pkg_fqn.is_extension();
    html! {
        <div style="display: inline-flex;">
            // Finding executions makes no sense when rendering an extension function.
            if !ext && !hide_find {
                <Link<Route, ExecutionQuery>
                    to={Route::ExecutionList}
                    query={ExecutionQuery { ffqn_prefix: Some(ffqn.ifc_fqn.to_string()), show_derived: true, ..Default::default() }}
                >
                    {ffqn.ifc_fqn.to_string()}
                </Link<Route, ExecutionQuery>>
            } else if *fully_qualified {
                {ffqn.ifc_fqn.to_string()}
            }
            if !hide_submit {
                <Link<Route> to={Route::ExecutionSubmit { ffqn: ffqn.clone() } }>
                    <Icon icon = { Icon::Play }/>
                </Link<Route>>
            } else if *fully_qualified {
                {"."}
            }
            <Link<Route, ExecutionQuery>
                    to={Route::ExecutionList}
                    query={ExecutionQuery { ffqn_prefix: Some(ffqn.to_string()), show_derived: true, ..Default::default() }}
                >
                {ffqn.function_name.to_string()}
            </Link<Route, ExecutionQuery>>
        </div>
    }
}
