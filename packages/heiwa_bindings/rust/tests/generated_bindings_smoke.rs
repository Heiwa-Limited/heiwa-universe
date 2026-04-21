use heiwa_bindings::route_decision_type::RouteDecision;

#[test]
fn generated_route_decision_type_is_importable() {
    let _ = std::any::type_name::<RouteDecision>();
}
