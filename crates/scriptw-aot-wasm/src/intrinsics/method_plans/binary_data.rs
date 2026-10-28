use super::*;

pub(super) const ARRAY_BUFFER_PROTOTYPE_MUTABLE: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("resize", StandardBuiltinId::ArrayBufferPrototypeResize),
    IntrinsicMethodSpec::named("transfer", StandardBuiltinId::ArrayBufferPrototypeTransfer),
    IntrinsicMethodSpec::named(
        "transferToFixedLength",
        StandardBuiltinId::ArrayBufferPrototypeTransferToFixedLength,
    ),
    IntrinsicMethodSpec::named(
        "transferToImmutable",
        StandardBuiltinId::ArrayBufferPrototypeTransferToImmutable,
    ),
    IntrinsicMethodSpec::named(
        "sliceToImmutable",
        StandardBuiltinId::ArrayBufferPrototypeSliceToImmutable,
    ),
];

pub(super) const DATA_VIEW_PROTOTYPE: &[IntrinsicMethodSpec] = &[
    IntrinsicMethodSpec::named("getUint8", StandardBuiltinId::DataViewPrototypeGetUint8),
    IntrinsicMethodSpec::named("setUint8", StandardBuiltinId::DataViewPrototypeSetUint8),
    IntrinsicMethodSpec::named("getInt8", StandardBuiltinId::DataViewPrototypeGetInt8),
    IntrinsicMethodSpec::named("setInt8", StandardBuiltinId::DataViewPrototypeSetInt8),
    IntrinsicMethodSpec::named("getUint16", StandardBuiltinId::DataViewPrototypeGetUint16),
    IntrinsicMethodSpec::named("setUint16", StandardBuiltinId::DataViewPrototypeSetUint16),
    IntrinsicMethodSpec::named("getInt16", StandardBuiltinId::DataViewPrototypeGetInt16),
    IntrinsicMethodSpec::named("setInt16", StandardBuiltinId::DataViewPrototypeSetInt16),
    IntrinsicMethodSpec::named("getUint32", StandardBuiltinId::DataViewPrototypeGetUint32),
    IntrinsicMethodSpec::named("setUint32", StandardBuiltinId::DataViewPrototypeSetUint32),
    IntrinsicMethodSpec::named("getInt32", StandardBuiltinId::DataViewPrototypeGetInt32),
    IntrinsicMethodSpec::named("setInt32", StandardBuiltinId::DataViewPrototypeSetInt32),
    IntrinsicMethodSpec::named("getFloat16", StandardBuiltinId::DataViewPrototypeGetFloat16),
    IntrinsicMethodSpec::named("setFloat16", StandardBuiltinId::DataViewPrototypeSetFloat16),
    IntrinsicMethodSpec::named("getFloat32", StandardBuiltinId::DataViewPrototypeGetFloat32),
    IntrinsicMethodSpec::named("setFloat32", StandardBuiltinId::DataViewPrototypeSetFloat32),
    IntrinsicMethodSpec::named("getFloat64", StandardBuiltinId::DataViewPrototypeGetFloat64),
    IntrinsicMethodSpec::named("setFloat64", StandardBuiltinId::DataViewPrototypeSetFloat64),
    IntrinsicMethodSpec::named(
        "getBigInt64",
        StandardBuiltinId::DataViewPrototypeGetBigInt64,
    ),
    IntrinsicMethodSpec::named(
        "setBigInt64",
        StandardBuiltinId::DataViewPrototypeSetBigInt64,
    ),
    IntrinsicMethodSpec::named(
        "getBigUint64",
        StandardBuiltinId::DataViewPrototypeGetBigUint64,
    ),
    IntrinsicMethodSpec::named(
        "setBigUint64",
        StandardBuiltinId::DataViewPrototypeSetBigUint64,
    ),
];
