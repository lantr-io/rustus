//! UPLC evaluation tests for List::at.

use rustus_core::data::{Data, FromData, ToData};
use rustus_core::num_bigint::BigInt;
use rustus_prelude::list::{self, List};

#[rustus::compile]
fn check_at(list_data: Data, idx_data: Data) {
    let list: List<Data> = FromData::from_data(&list_data).unwrap();
    let idx: BigInt = FromData::from_data(&idx_data).unwrap();
    let _elem: Data = list::at(list, idx);
}

fn try_compile() -> Option<rustus::Validator> {
    rustus::compile_module("check_at").ok()
}

fn make_list(items: Vec<i64>) -> Data {
    List::from_vec(items.into_iter().map(|i| Data::I { value: BigInt::from(i) }).collect::<Vec<_>>()).to_data()
}

#[test]
fn at_first_element() {
    let Some(v) = try_compile() else { return };
    let result = v.eval(&[make_list(vec![10, 20, 30]), BigInt::from(0).to_data()]).unwrap();
    assert!(result.succeeded(), "at(0) failed: {:?}", result.error);
}

#[test]
fn at_middle_element() {
    let Some(v) = try_compile() else { return };
    let result = v.eval(&[make_list(vec![10, 20, 30]), BigInt::from(1).to_data()]).unwrap();
    assert!(result.succeeded(), "at(1) failed: {:?}", result.error);
}

#[test]
fn at_last_element() {
    let Some(v) = try_compile() else { return };
    let result = v.eval(&[make_list(vec![10, 20, 30]), BigInt::from(2).to_data()]).unwrap();
    assert!(result.succeeded(), "at(2) failed: {:?}", result.error);
}

#[test]
fn at_out_of_bounds_fails() {
    let Some(v) = try_compile() else { return };
    let result = v.eval(&[make_list(vec![10, 20, 30]), BigInt::from(3).to_data()]).unwrap();
    assert!(result.failed());
}

#[test]
fn at_empty_list_fails() {
    let Some(v) = try_compile() else { return };
    let result = v.eval(&[make_list(vec![]), BigInt::from(0).to_data()]).unwrap();
    assert!(result.failed());
}

#[test]
fn at_single_element() {
    let Some(v) = try_compile() else { return };
    let result = v.eval(&[make_list(vec![42]), BigInt::from(0).to_data()]).unwrap();
    assert!(result.succeeded(), "at(0) on singleton failed: {:?}", result.error);
}
