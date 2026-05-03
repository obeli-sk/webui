use yew::prelude::*;

#[component(NotFound)]
pub fn not_found() -> Html {
    html! {
        <div>
            {"The URL was not found"}
        </div>
    }
}
