use rest_types::components::ParameterTypeLite;
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct FunctionSignatureProps {
    pub params: Vec<ParameterTypeLite>,
    pub return_type: String,
}
#[component(FunctionSignature)]
pub fn function_signature(
    FunctionSignatureProps {
        params,
        return_type,
    }: &FunctionSignatureProps,
) -> Html {
    html! {<>
        {"func ("}
            <FunctionParameterList params = {params.clone()} />
        {")"}
        {" -> "}
        { return_type }
    </>}
}

#[derive(Properties, PartialEq)]
pub struct FunctionParameterListProps {
    pub params: Vec<ParameterTypeLite>,
}
#[component(FunctionParameterList)]
pub fn function_parameter_list(
    FunctionParameterListProps { params }: &FunctionParameterListProps,
) -> Html {
    params
        .iter()
        .enumerate()
        .map(|(idx, param)| {
            let ty = &param.wit_type;
            html! {<>
                if idx > 0 {
                    {", "}
                }
                { format!("{}: {}", param.name, ty) }
            </>}
        })
        .collect()
}
