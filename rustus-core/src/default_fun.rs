use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefaultFun {
    AddInteger,
    SubtractInteger,
    MultiplyInteger,
    EqualsInteger,
    LessThanInteger,
    LessThanEqualsInteger,
    IfThenElse,
    EqualsData,
    EqualsByteString,
    EqualsString,
    ConstrData,
    UnConstrData,
    HeadList,
    TailList,
    NullList,
    MkCons,
    MkNilData,
    Trace,
}
