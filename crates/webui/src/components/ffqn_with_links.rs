use crate::tree::Icon;
use crate::{app::Route, components::execution_list_page::ExecutionQuery, grpc::ffqn::FunctionFqn};
use yew::prelude::*;
use yew_router::prelude::Link;

#[derive(Properties, PartialEq)]
pub struct FfqnWithLinksProps {
    pub ffqn: FunctionFqn,
    #[prop_or_default]
    pub fully_qualified: bool,
    #[prop_or_default]
    pub hide_submit: bool,
}
#[component(FfqnWithLinks)]
pub fn ffqn_with_links(
    FfqnWithLinksProps {
        ffqn,
        fully_qualified,
        hide_submit,
    }: &FfqnWithLinksProps,
) -> Html {
    let ext = ffqn.ifc_fqn.pkg_fqn.is_extension();
    html! {
        <span class="ffqn">
            if *fully_qualified {
                if !ext {
                    // searchable interface link
                    <Link<Route, ExecutionQuery>
                        to={Route::ExecutionList}
                        query={ExecutionQuery { ffqn_prefix: Some(ffqn.ifc_fqn.to_string()), show_derived: true, ..Default::default() }}
                    >
                        <span class="ffqn-interface">{ffqn.ifc_fqn.to_string()}</span>
                    </Link<Route, ExecutionQuery>>
                } else {
                    // extension interface, not searchable
                    <span class="ffqn-interface">{ffqn.ifc_fqn.to_string()}</span>
                }
                <span class="ffqn-separator">{"."}</span>
            }
            <Link<Route, ExecutionQuery>
                to={Route::ExecutionList}
                query={ExecutionQuery { ffqn_prefix: Some(ffqn.to_string()), show_derived: true, ..Default::default() }}
            >
                <span class="ffqn-function-name">{ffqn.function_name.to_string()}</span>
            </Link<Route, ExecutionQuery>>
            if !hide_submit {
                <span class="ffqn-submit" title="Run this function">
                    <Link<Route> to={Route::ExecutionSubmit { ffqn: ffqn.clone() } }>
                        { Html::from(Icon::Play) }
                    </Link<Route>>
                </span>
            }
        </span>
    }
}
