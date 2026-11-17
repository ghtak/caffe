use espr::validate::Validate as _;
use espr_derive::Validate;

#[derive(Validate)]
struct Request<T>
where
    T: Clone,
{
    _value: T,
}

#[test]
fn derived_validation_succeeds_for_generic_request() {
    let request = Request { _value: 42 };

    // 제네릭과 where 절이 있는 외부 타입에도 검증 구현이 생성된다.
    let result: Result<(), std::convert::Infallible> = request.validate();
    assert_eq!(result, Ok(()));
}
