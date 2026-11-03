use super::*;

impl<'a> ScriptLowerer<'a> {
    pub(super) fn empty_object_shape() -> HeapShape {
        HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: None,
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        })
    }

    pub(super) fn raw_json_object_shape() -> HeapShape {
        HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: None,
            properties: BTreeMap::from([(
                "rawJSON".to_string(),
                ObjectShapeProperty::Data(ValueInfo::new(ValueKind::String)),
            )]),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        })
    }

    pub(super) fn fresh_constructed_instance_info() -> ValueInfo {
        ValueInfo {
            kind: ValueKind::Object,
            possible_kinds: KindSet::from_kind(ValueKind::Object),
            heap_shape: Some(Box::new(Self::empty_object_shape())),
            function_targets: FunctionTargetKnowledge::none(),
        }
    }

    pub(super) fn fresh_constructed_array_instance_info() -> ValueInfo {
        ValueInfo {
            kind: ValueKind::Array,
            possible_kinds: KindSet::from_kind(ValueKind::Array),
            heap_shape: Some(Box::new(HeapShape::Array(ArrayShape::default()))),
            function_targets: FunctionTargetKnowledge::none(),
        }
    }

    pub(super) fn shadow_realm_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, builtin) in SHADOW_REALM_PROTOTYPE_METHODS {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    builtin.function_id(),
                    false,
                )),
            );
        }
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(ValueInfo::new(ValueKind::String)),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::ShadowRealmConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn shadow_realm_instance_info() -> ValueInfo {
        Self::with_instance_prototype(
            Self::fresh_constructed_instance_info(),
            Some(Self::shadow_realm_prototype_shape()),
        )
    }

    pub(super) fn array_buffer_instance_shape() -> Box<HeapShape> {
        Self::array_buffer_instance_shape_with_prototype(Self::array_buffer_prototype_shape())
    }

    pub(super) fn shared_array_buffer_instance_shape() -> Box<HeapShape> {
        Self::array_buffer_instance_shape_with_prototype(Self::shared_array_buffer_prototype_shape())
    }

    pub(super) fn array_buffer_instance_shape_with_prototype(
        prototype: Box<HeapShape>,
    ) -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(prototype),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn array_buffer_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        properties.insert(
            "byteLength".to_string(),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::ArrayBufferPrototypeByteLengthGetter
                        .function_id(),
                }),
                setter: None,
            },
        );
        properties.insert(
            "constructor".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ArrayBufferConstructor.function_id(),
                true,
            )),
        );
        properties.insert(
            SHARED_ARRAY_BUFFER_NAME.to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::SharedArrayBufferConstructor.function_id(),
                true,
            )),
        );
        properties.insert(
            "detached".to_string(),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::ArrayBufferPrototypeDetachedGetter
                        .function_id(),
                }),
                setter: None,
            },
        );
        properties.insert(
            "maxByteLength".to_string(),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::ArrayBufferPrototypeMaxByteLengthGetter
                        .function_id(),
                }),
                setter: None,
            },
        );
        properties.insert(
            "immutable".to_string(),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::ArrayBufferPrototypeImmutableGetter
                        .function_id(),
                }),
                setter: None,
            },
        );
        properties.insert(
            "resizable".to_string(),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::ArrayBufferPrototypeResizableGetter
                        .function_id(),
                }),
                setter: None,
            },
        );
        properties.insert(
            "resize".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ArrayBufferPrototypeResize.function_id(),
                false,
            )),
        );
        properties.insert(
            "slice".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ArrayBufferPrototypeSlice.function_id(),
                false,
            )),
        );
        properties.insert(
            "sliceToImmutable".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ArrayBufferPrototypeSliceToImmutable.function_id(),
                false,
            )),
        );
        properties.insert(
            "transfer".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ArrayBufferPrototypeTransfer.function_id(),
                false,
            )),
        );
        properties.insert(
            "transferToFixedLength".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ArrayBufferPrototypeTransferToFixedLength.function_id(),
                false,
            )),
        );
        properties.insert(
            "transferToImmutable".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ArrayBufferPrototypeTransferToImmutable.function_id(),
                false,
            )),
        );
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info("ArrayBuffer")),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::ArrayBufferConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn shared_array_buffer_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        properties.insert(
            "byteLength".to_string(),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::SharedArrayBufferPrototypeByteLengthGetter
                        .function_id(),
                }),
                setter: None,
            },
        );
        properties.insert(
            "maxByteLength".to_string(),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::SharedArrayBufferPrototypeMaxByteLengthGetter
                        .function_id(),
                }),
                setter: None,
            },
        );
        properties.insert(
            "growable".to_string(),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::SharedArrayBufferPrototypeGrowableGetter
                        .function_id(),
                }),
                setter: None,
            },
        );
        properties.insert(
            "grow".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::SharedArrayBufferPrototypeGrow.function_id(),
                false,
            )),
        );
        properties.insert(
            "slice".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::SharedArrayBufferPrototypeSlice.function_id(),
                false,
            )),
        );
        properties.insert(
            "constructor".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::SharedArrayBufferConstructor.function_id(),
                true,
            )),
        );
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info("SharedArrayBuffer")),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::SharedArrayBufferConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn synthetic_realm_global_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn synthetic_realm_record_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        properties.insert(
            "global".to_string(),
            ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                Self::synthetic_realm_global_shape(),
            ))),
        );
        properties.insert(
            REALM_EVAL_SCRIPT_METHOD_NAME.to_string(),
            ObjectShapeProperty::Data(Self::host_function_value_info(
                HostBuiltinId::RealmEvalScript,
            )),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn data_view_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        properties.insert(
            "buffer".to_string(),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::DataViewPrototypeBufferGetter.function_id(),
                }),
                setter: None,
            },
        );
        properties.insert(
            "byteLength".to_string(),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::DataViewPrototypeByteLengthGetter.function_id(),
                }),
                setter: None,
            },
        );
        properties.insert(
            "byteOffset".to_string(),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::DataViewPrototypeByteOffsetGetter.function_id(),
                }),
                setter: None,
            },
        );
        properties.insert(
            "getUint8".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeGetUint8.function_id(),
                false,
            )),
        );
        properties.insert(
            "setUint8".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeSetUint8.function_id(),
                false,
            )),
        );
        properties.insert(
            "getInt8".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeGetInt8.function_id(),
                false,
            )),
        );
        properties.insert(
            "setInt8".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeSetInt8.function_id(),
                false,
            )),
        );
        properties.insert(
            "getUint16".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeGetUint16.function_id(),
                false,
            )),
        );
        properties.insert(
            "setUint16".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeSetUint16.function_id(),
                false,
            )),
        );
        properties.insert(
            "getInt16".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeGetInt16.function_id(),
                false,
            )),
        );
        properties.insert(
            "setInt16".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeSetInt16.function_id(),
                false,
            )),
        );
        properties.insert(
            "getUint32".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeGetUint32.function_id(),
                false,
            )),
        );
        properties.insert(
            "setUint32".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeSetUint32.function_id(),
                false,
            )),
        );
        properties.insert(
            "getInt32".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeGetInt32.function_id(),
                false,
            )),
        );
        properties.insert(
            "setInt32".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeSetInt32.function_id(),
                false,
            )),
        );
        properties.insert(
            "getFloat16".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeGetFloat16.function_id(),
                false,
            )),
        );
        properties.insert(
            "setFloat16".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeSetFloat16.function_id(),
                false,
            )),
        );
        properties.insert(
            "getFloat32".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeGetFloat32.function_id(),
                false,
            )),
        );
        properties.insert(
            "setFloat32".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeSetFloat32.function_id(),
                false,
            )),
        );
        properties.insert(
            "getFloat64".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeGetFloat64.function_id(),
                false,
            )),
        );
        properties.insert(
            "setFloat64".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeSetFloat64.function_id(),
                false,
            )),
        );
        properties.insert(
            "getBigInt64".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeGetBigInt64.function_id(),
                false,
            )),
        );
        properties.insert(
            "setBigInt64".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeSetBigInt64.function_id(),
                false,
            )),
        );
        properties.insert(
            "getBigUint64".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeGetBigUint64.function_id(),
                false,
            )),
        );
        properties.insert(
            "setBigUint64".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DataViewPrototypeSetBigUint64.function_id(),
                false,
            )),
        );
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info("DataView")),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::DataViewConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn data_view_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::data_view_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn promise_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        properties.insert(
            "constructor".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::PromiseConstructor.function_id(),
                true,
            )),
        );
        properties.insert(
            "then".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::PromisePrototypeThen.function_id(),
                false,
            )),
        );
        properties.insert(
            "catch".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::PromisePrototypeCatch.function_id(),
                false,
            )),
        );
        properties.insert(
            "finally".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::PromisePrototypeFinally.function_id(),
                false,
            )),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::PromiseConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn promise_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::promise_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn map_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, builtin) in [
            ("clear", StandardBuiltinId::MapPrototypeClear),
            ("delete", StandardBuiltinId::MapPrototypeDelete),
            ("forEach", StandardBuiltinId::MapPrototypeForEach),
            ("get", StandardBuiltinId::MapPrototypeGet),
            ("getOrInsert", StandardBuiltinId::MapPrototypeGetOrInsert),
            (
                "getOrInsertComputed",
                StandardBuiltinId::MapPrototypeGetOrInsertComputed,
            ),
            ("has", StandardBuiltinId::MapPrototypeHas),
            ("keys", StandardBuiltinId::MapPrototypeKeys),
            ("set", StandardBuiltinId::MapPrototypeSet),
            ("values", StandardBuiltinId::MapPrototypeValues),
            ("entries", StandardBuiltinId::MapPrototypeEntries),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    builtin.function_id(),
                    false,
                )),
            );
        }
        // 24.1.3.12: `%Map.prototype%[@@iterator]` is the same function object
        // as `%Map.prototype%.entries`. Lifted out of the string-keyed loop
        // above because its key is a symbol, not a string.
        properties.insert(
            shape_namespace_key(WellKnownSymbol::Iterator),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::MapPrototypeEntries.function_id(),
                false,
            )),
        );
        properties.insert(
            "size".to_string(),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::MapPrototypeSizeGetter.function_id(),
                }),
                setter: None,
            },
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(StandardBuiltinId::MapConstructor),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn map_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::map_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn weak_map_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, builtin) in [
            ("delete", StandardBuiltinId::WeakMapPrototypeDelete),
            ("get", StandardBuiltinId::WeakMapPrototypeGet),
            (
                "getOrInsert",
                StandardBuiltinId::WeakMapPrototypeGetOrInsert,
            ),
            (
                "getOrInsertComputed",
                StandardBuiltinId::WeakMapPrototypeGetOrInsertComputed,
            ),
            ("has", StandardBuiltinId::WeakMapPrototypeHas),
            ("set", StandardBuiltinId::WeakMapPrototypeSet),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    builtin.function_id(),
                    false,
                )),
            );
        }
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::WeakMapConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn weak_map_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::weak_map_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn weak_set_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        properties.insert(
            "constructor".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::WeakSetConstructor.function_id(),
                true,
            )),
        );
        for (name, builtin) in [
            ("add", StandardBuiltinId::WeakSetPrototypeAdd),
            ("delete", StandardBuiltinId::WeakSetPrototypeDelete),
            ("has", StandardBuiltinId::WeakSetPrototypeHas),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    builtin.function_id(),
                    false,
                )),
            );
        }
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info("WeakSet")),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::WeakSetConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn weak_set_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::weak_set_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn weak_ref_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        properties.insert(
            "constructor".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::WeakRefConstructor.function_id(),
                true,
            )),
        );
        properties.insert(
            "deref".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::WeakRefPrototypeDeref.function_id(),
                false,
            )),
        );
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info("WeakRef")),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::WeakRefConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn weak_ref_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::weak_ref_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn finalization_registry_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        properties.insert(
            "constructor".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::FinalizationRegistryConstructor.function_id(),
                true,
            )),
        );
        for (name, builtin) in [
            (
                "register",
                StandardBuiltinId::FinalizationRegistryPrototypeRegister,
            ),
            (
                "unregister",
                StandardBuiltinId::FinalizationRegistryPrototypeUnregister,
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    builtin.function_id(),
                    false,
                )),
            );
        }
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info("FinalizationRegistry")),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::FinalizationRegistryConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn finalization_registry_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::finalization_registry_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn map_iterator_instance_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        properties.insert(
            "next".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::MapIteratorNext,
            )),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::UntrackedIntrinsicPrototype,
            prototype: Some(Self::iterator_prototype_shape()),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn set_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, builtin) in [
            ("add", StandardBuiltinId::SetPrototypeAdd),
            ("clear", StandardBuiltinId::SetPrototypeClear),
            ("delete", StandardBuiltinId::SetPrototypeDelete),
            ("difference", StandardBuiltinId::SetPrototypeDifference),
            ("forEach", StandardBuiltinId::SetPrototypeForEach),
            ("has", StandardBuiltinId::SetPrototypeHas),
            ("intersection", StandardBuiltinId::SetPrototypeIntersection),
            (
                "isDisjointFrom",
                StandardBuiltinId::SetPrototypeIsDisjointFrom,
            ),
            ("isSubsetOf", StandardBuiltinId::SetPrototypeIsSubsetOf),
            ("isSupersetOf", StandardBuiltinId::SetPrototypeIsSupersetOf),
            (
                "symmetricDifference",
                StandardBuiltinId::SetPrototypeSymmetricDifference,
            ),
            ("union", StandardBuiltinId::SetPrototypeUnion),
            ("values", StandardBuiltinId::SetPrototypeValues),
            ("keys", StandardBuiltinId::SetPrototypeValues),
            ("entries", StandardBuiltinId::SetPrototypeEntries),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    builtin.function_id(),
                    false,
                )),
            );
        }
        // 24.2.4.11: `%Set.prototype%[@@iterator]` is the same function object
        // as `%Set.prototype%.values`. Lifted out of the string-keyed loop
        // above because its key is a symbol, not a string.
        properties.insert(
            shape_namespace_key(WellKnownSymbol::Iterator),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::SetPrototypeValues.function_id(),
                false,
            )),
        );
        properties.insert(
            "size".to_string(),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::SetPrototypeSizeGetter.function_id(),
                }),
                setter: None,
            },
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(StandardBuiltinId::SetConstructor),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn set_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::set_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn set_iterator_instance_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        properties.insert(
            "next".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::SetIteratorNext,
            )),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::UntrackedIntrinsicPrototype,
            prototype: Some(Self::iterator_prototype_shape()),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn date_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, builtin) in [
            ("getTime", StandardBuiltinId::DatePrototypeGetTime),
            ("setTime", StandardBuiltinId::DatePrototypeSetTime),
            ("valueOf", StandardBuiltinId::DatePrototypeValueOf),
            ("getFullYear", StandardBuiltinId::DatePrototypeGetFullYear),
            (
                "getUTCFullYear",
                StandardBuiltinId::DatePrototypeGetUtcFullYear,
            ),
            ("getMonth", StandardBuiltinId::DatePrototypeGetMonth),
            ("getUTCMonth", StandardBuiltinId::DatePrototypeGetUtcMonth),
            ("getDate", StandardBuiltinId::DatePrototypeGetDate),
            ("getUTCDate", StandardBuiltinId::DatePrototypeGetUtcDate),
            ("getDay", StandardBuiltinId::DatePrototypeGetDay),
            ("getUTCDay", StandardBuiltinId::DatePrototypeGetUtcDay),
            ("getHours", StandardBuiltinId::DatePrototypeGetHours),
            ("getUTCHours", StandardBuiltinId::DatePrototypeGetUtcHours),
            ("getMinutes", StandardBuiltinId::DatePrototypeGetMinutes),
            (
                "getUTCMinutes",
                StandardBuiltinId::DatePrototypeGetUtcMinutes,
            ),
            ("getSeconds", StandardBuiltinId::DatePrototypeGetSeconds),
            (
                "getUTCSeconds",
                StandardBuiltinId::DatePrototypeGetUtcSeconds,
            ),
            (
                "getMilliseconds",
                StandardBuiltinId::DatePrototypeGetMilliseconds,
            ),
            (
                "getUTCMilliseconds",
                StandardBuiltinId::DatePrototypeGetUtcMilliseconds,
            ),
            (
                "getTimezoneOffset",
                StandardBuiltinId::DatePrototypeGetTimezoneOffset,
            ),
            ("getYear", StandardBuiltinId::DatePrototypeGetYear),
            ("setYear", StandardBuiltinId::DatePrototypeSetYear),
            ("setFullYear", StandardBuiltinId::DatePrototypeSetFullYear),
            (
                "setUTCFullYear",
                StandardBuiltinId::DatePrototypeSetUtcFullYear,
            ),
            ("setMonth", StandardBuiltinId::DatePrototypeSetMonth),
            ("setUTCMonth", StandardBuiltinId::DatePrototypeSetUtcMonth),
            ("setDate", StandardBuiltinId::DatePrototypeSetDate),
            ("setUTCDate", StandardBuiltinId::DatePrototypeSetUtcDate),
            ("setHours", StandardBuiltinId::DatePrototypeSetHours),
            ("setUTCHours", StandardBuiltinId::DatePrototypeSetUtcHours),
            ("setMinutes", StandardBuiltinId::DatePrototypeSetMinutes),
            (
                "setUTCMinutes",
                StandardBuiltinId::DatePrototypeSetUtcMinutes,
            ),
            ("setSeconds", StandardBuiltinId::DatePrototypeSetSeconds),
            (
                "setUTCSeconds",
                StandardBuiltinId::DatePrototypeSetUtcSeconds,
            ),
            (
                "setMilliseconds",
                StandardBuiltinId::DatePrototypeSetMilliseconds,
            ),
            (
                "setUTCMilliseconds",
                StandardBuiltinId::DatePrototypeSetUtcMilliseconds,
            ),
            ("toISOString", StandardBuiltinId::DatePrototypeToIsoString),
            ("toJSON", StandardBuiltinId::DatePrototypeToJson),
            ("toDateString", StandardBuiltinId::DatePrototypeToDateString),
            (
                "toLocaleDateString",
                StandardBuiltinId::DatePrototypeToLocaleDateString,
            ),
            (
                "toLocaleString",
                StandardBuiltinId::DatePrototypeToLocaleString,
            ),
            (
                "toLocaleTimeString",
                StandardBuiltinId::DatePrototypeToLocaleTimeString,
            ),
            (
                "toTemporalInstant",
                StandardBuiltinId::DatePrototypeToTemporalInstant,
            ),
            ("toTimeString", StandardBuiltinId::DatePrototypeToTimeString),
            ("toString", StandardBuiltinId::DatePrototypeToString),
            ("toUTCString", StandardBuiltinId::DatePrototypeToUtcString),
            ("toGMTString", StandardBuiltinId::DatePrototypeToUtcString),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    builtin.function_id(),
                    false,
                )),
            );
        }
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToPrimitive),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::DatePrototypeToPrimitive.function_id(),
                false,
            )),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(StandardBuiltinId::DateConstructor),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn date_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::date_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn temporal_instant_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, getter) in [
            (
                "epochMilliseconds",
                StandardBuiltinId::TemporalInstantPrototypeEpochMillisecondsGetter,
            ),
            (
                "epochNanoseconds",
                StandardBuiltinId::TemporalInstantPrototypeEpochNanosecondsGetter,
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Accessor {
                    getter: Some(ObjectAccessorShape {
                        function_id: getter.function_id(),
                    }),
                    setter: None,
                },
            );
        }
        for builtin in [
            StandardBuiltinId::TemporalInstantPrototypeAdd,
            StandardBuiltinId::TemporalInstantPrototypeSubtract,
            StandardBuiltinId::TemporalInstantPrototypeRound,
            StandardBuiltinId::TemporalInstantPrototypeUntil,
            StandardBuiltinId::TemporalInstantPrototypeSince,
            StandardBuiltinId::TemporalInstantPrototypeToLocaleString,
        ] {
            properties.insert(
                builtin
                    .native_function_name()
                    .expect("Instant method name")
                    .to_owned(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    builtin.function_id(),
                    false,
                )),
            );
        }
        properties.insert(
            "equals".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalInstantPrototypeEquals.function_id(),
                false,
            )),
        );
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info("Temporal.Instant")),
        );
        properties.insert(
            "toString".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalInstantPrototypeToString.function_id(),
                false,
            )),
        );
        // `toJSON` shares `toString`'s emitter but never its function object:
        // `Object.getOwnPropertyDescriptor(Temporal.Instant.prototype, "toJSON")
        // .value === Temporal.Instant.prototype.toString` must be false, and
        // `toJSON.name` must be `"toJSON"`.
        properties.insert(
            "toJSON".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalInstantPrototypeToJson.function_id(),
                false,
            )),
        );
        properties.insert(
            "valueOf".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalInstantPrototypeValueOf.function_id(),
                false,
            )),
        );
        properties.insert(
            "toZonedDateTimeISO".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalInstantPrototypeToZonedDateTimeIso.function_id(),
                false,
            )),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::TemporalInstantConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn temporal_instant_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::temporal_instant_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn intl_locale_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, getter) in [
            (
                "language",
                StandardBuiltinId::IntlLocalePrototypeLanguageGetter,
            ),
            ("script", StandardBuiltinId::IntlLocalePrototypeScriptGetter),
            ("region", StandardBuiltinId::IntlLocalePrototypeRegionGetter),
            (
                "baseName",
                StandardBuiltinId::IntlLocalePrototypeBaseNameGetter,
            ),
            (
                "calendar",
                StandardBuiltinId::IntlLocalePrototypeCalendarGetter,
            ),
            (
                "collation",
                StandardBuiltinId::IntlLocalePrototypeCollationGetter,
            ),
            (
                "firstDayOfWeek",
                StandardBuiltinId::IntlLocalePrototypeFirstDayOfWeekGetter,
            ),
            (
                "hourCycle",
                StandardBuiltinId::IntlLocalePrototypeHourCycleGetter,
            ),
            (
                "caseFirst",
                StandardBuiltinId::IntlLocalePrototypeCaseFirstGetter,
            ),
            (
                "numeric",
                StandardBuiltinId::IntlLocalePrototypeNumericGetter,
            ),
            (
                "numberingSystem",
                StandardBuiltinId::IntlLocalePrototypeNumberingSystemGetter,
            ),
            (
                "variants",
                StandardBuiltinId::IntlLocalePrototypeVariantsGetter,
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Accessor {
                    getter: Some(ObjectAccessorShape {
                        function_id: getter.function_id(),
                    }),
                    setter: None,
                },
            );
        }
        for (name, method) in [
            ("toString", StandardBuiltinId::IntlLocalePrototypeToString),
            ("maximize", StandardBuiltinId::IntlLocalePrototypeMaximize),
            ("minimize", StandardBuiltinId::IntlLocalePrototypeMinimize),
            (
                "getCalendars",
                StandardBuiltinId::IntlLocalePrototypeGetCalendars,
            ),
            (
                "getCollations",
                StandardBuiltinId::IntlLocalePrototypeGetCollations,
            ),
            (
                "getTimeZones",
                StandardBuiltinId::IntlLocalePrototypeGetTimeZones,
            ),
            (
                "getNumberingSystems",
                StandardBuiltinId::IntlLocalePrototypeGetNumberingSystems,
            ),
            (
                "getHourCycles",
                StandardBuiltinId::IntlLocalePrototypeGetHourCycles,
            ),
            (
                "getTextInfo",
                StandardBuiltinId::IntlLocalePrototypeGetTextInfo,
            ),
            (
                "getWeekInfo",
                StandardBuiltinId::IntlLocalePrototypeGetWeekInfo,
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    method.function_id(),
                    false,
                )),
            );
        }
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info("Intl.Locale")),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::IntlLocaleConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn intl_locale_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::intl_locale_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn temporal_zoned_date_time_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, getter) in [
            (
                "epochMilliseconds",
                StandardBuiltinId::TemporalZonedDateTimePrototypeEpochMillisecondsGetter,
            ),
            (
                "epochNanoseconds",
                StandardBuiltinId::TemporalZonedDateTimePrototypeEpochNanosecondsGetter,
            ),
            (
                "offset",
                StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetGetter,
            ),
            (
                "offsetNanoseconds",
                StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetNanosecondsGetter,
            ),
            (
                "timeZoneId",
                StandardBuiltinId::TemporalZonedDateTimePrototypeTimeZoneIdGetter,
            ),
            (
                "calendarId",
                StandardBuiltinId::TemporalZonedDateTimePrototypeCalendarIdGetter,
            ),
            // `era`/`eraYear` are written between `calendarId` and `year` to
            // match the spec's order and the order
            // `temporal_plain_date_time_prototype_shape` already uses, but the
            // order in *this* list is not observable: `properties` is a
            // `BTreeMap<String, _>`, so it is re-keyed lexicographically the
            // moment it is inserted. Observable property order is decided by
            // the define-property sequence in
            // `install_temporal_zoned_date_time_constructor_intrinsics`
            // (`intrinsics/temporal.rs`), and that is where the ordering
            // requirement is recorded. What this list must agree with that
            // loop on is *membership*: a shape entry with no accessor is a
            // property the shape promises and the prototype does not have.
            (
                "era",
                StandardBuiltinId::TemporalZonedDateTimePrototypeEraGetter,
            ),
            (
                "eraYear",
                StandardBuiltinId::TemporalZonedDateTimePrototypeEraYearGetter,
            ),
            (
                "year",
                StandardBuiltinId::TemporalZonedDateTimePrototypeYearGetter,
            ),
            (
                "month",
                StandardBuiltinId::TemporalZonedDateTimePrototypeMonthGetter,
            ),
            (
                "monthCode",
                StandardBuiltinId::TemporalZonedDateTimePrototypeMonthCodeGetter,
            ),
            (
                "day",
                StandardBuiltinId::TemporalZonedDateTimePrototypeDayGetter,
            ),
            (
                "hour",
                StandardBuiltinId::TemporalZonedDateTimePrototypeHourGetter,
            ),
            (
                "minute",
                StandardBuiltinId::TemporalZonedDateTimePrototypeMinuteGetter,
            ),
            (
                "second",
                StandardBuiltinId::TemporalZonedDateTimePrototypeSecondGetter,
            ),
            (
                "millisecond",
                StandardBuiltinId::TemporalZonedDateTimePrototypeMillisecondGetter,
            ),
            (
                "microsecond",
                StandardBuiltinId::TemporalZonedDateTimePrototypeMicrosecondGetter,
            ),
            (
                "nanosecond",
                StandardBuiltinId::TemporalZonedDateTimePrototypeNanosecondGetter,
            ),
            (
                "dayOfWeek",
                StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfWeekGetter,
            ),
            (
                "dayOfYear",
                StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfYearGetter,
            ),
            (
                "weekOfYear",
                StandardBuiltinId::TemporalZonedDateTimePrototypeWeekOfYearGetter,
            ),
            (
                "yearOfWeek",
                StandardBuiltinId::TemporalZonedDateTimePrototypeYearOfWeekGetter,
            ),
            (
                "daysInWeek",
                StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInWeekGetter,
            ),
            (
                "daysInMonth",
                StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInMonthGetter,
            ),
            (
                "daysInYear",
                StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInYearGetter,
            ),
            (
                "hoursInDay",
                StandardBuiltinId::TemporalZonedDateTimePrototypeHoursInDayGetter,
            ),
            (
                "monthsInYear",
                StandardBuiltinId::TemporalZonedDateTimePrototypeMonthsInYearGetter,
            ),
            (
                "inLeapYear",
                StandardBuiltinId::TemporalZonedDateTimePrototypeInLeapYearGetter,
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Accessor {
                    getter: Some(ObjectAccessorShape {
                        function_id: getter.function_id(),
                    }),
                    setter: None,
                },
            );
        }
        // Every data-property method of this prototype, read out of the one
        // table both crates iterate
        // (`names::TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_METHODS`). Batch 6 shipped
        // its five new members literally here AND in
        // `install_temporal_zoned_date_time_constructor_intrinsics`, with a
        // comment in each saying they must agree; that agreement is now
        // structural rather than remembered. `equals`, `toInstant`,
        // `withTimeZone` and `toPlainDateTime` were still spelled out here as
        // four separate `properties.insert` calls identical to this loop body,
        // duplicating four more members against the installer — they are in the
        // table now, at the head of it, in the order the installer used. See the
        // const's doc for what a divergence costs.
        for (name, builtin) in TEMPORAL_ZONED_DATE_TIME_PROTOTYPE_METHODS {
            properties.insert(
                (*name).to_string(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    builtin.function_id(),
                    false,
                )),
            );
        }
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info("Temporal.ZonedDateTime")),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::TemporalZonedDateTimeConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn temporal_zoned_date_time_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::temporal_zoned_date_time_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    /// Temporal proposal 3.3: the `Temporal.PlainDate.prototype` shape. The
    /// order here mirrors `install_temporal_plain_date_constructor_intrinsics`,
    /// because property order is observable through `Object.keys`.
    pub(super) fn temporal_plain_date_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, getter) in [
            (
                "calendarId",
                StandardBuiltinId::TemporalPlainDatePrototypeCalendarIdGetter,
            ),
            (
                "era",
                StandardBuiltinId::TemporalPlainDatePrototypeEraGetter,
            ),
            (
                "eraYear",
                StandardBuiltinId::TemporalPlainDatePrototypeEraYearGetter,
            ),
            (
                "year",
                StandardBuiltinId::TemporalPlainDatePrototypeYearGetter,
            ),
            (
                "month",
                StandardBuiltinId::TemporalPlainDatePrototypeMonthGetter,
            ),
            (
                "monthCode",
                StandardBuiltinId::TemporalPlainDatePrototypeMonthCodeGetter,
            ),
            (
                "day",
                StandardBuiltinId::TemporalPlainDatePrototypeDayGetter,
            ),
            (
                "dayOfWeek",
                StandardBuiltinId::TemporalPlainDatePrototypeDayOfWeekGetter,
            ),
            (
                "dayOfYear",
                StandardBuiltinId::TemporalPlainDatePrototypeDayOfYearGetter,
            ),
            (
                "weekOfYear",
                StandardBuiltinId::TemporalPlainDatePrototypeWeekOfYearGetter,
            ),
            (
                "yearOfWeek",
                StandardBuiltinId::TemporalPlainDatePrototypeYearOfWeekGetter,
            ),
            (
                "daysInWeek",
                StandardBuiltinId::TemporalPlainDatePrototypeDaysInWeekGetter,
            ),
            (
                "daysInMonth",
                StandardBuiltinId::TemporalPlainDatePrototypeDaysInMonthGetter,
            ),
            (
                "daysInYear",
                StandardBuiltinId::TemporalPlainDatePrototypeDaysInYearGetter,
            ),
            (
                "monthsInYear",
                StandardBuiltinId::TemporalPlainDatePrototypeMonthsInYearGetter,
            ),
            (
                "inLeapYear",
                StandardBuiltinId::TemporalPlainDatePrototypeInLeapYearGetter,
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Accessor {
                    getter: Some(ObjectAccessorShape {
                        function_id: getter.function_id(),
                    }),
                    setter: None,
                },
            );
        }
        properties.insert(
            "with".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalPlainDatePrototypeWith.function_id(),
                false,
            )),
        );
        properties.insert(
            "withCalendar".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalPlainDatePrototypeWithCalendar.function_id(),
                false,
            )),
        );
        properties.insert(
            "equals".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalPlainDatePrototypeEquals.function_id(),
                false,
            )),
        );
        properties.insert(
            "toString".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalPlainDatePrototypeToString.function_id(),
                false,
            )),
        );
        properties.insert(
            "toJSON".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalPlainDatePrototypeToJson.function_id(),
                false,
            )),
        );
        properties.insert(
            "toLocaleString".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalPlainDatePrototypeToLocaleString.function_id(),
                false,
            )),
        );
        properties.insert(
            "valueOf".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalPlainDatePrototypeValueOf.function_id(),
                false,
            )),
        );
        properties.insert(
            "add".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalPlainDatePrototypeAdd.function_id(),
                false,
            )),
        );
        properties.insert(
            "subtract".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalPlainDatePrototypeSubtract.function_id(),
                false,
            )),
        );
        properties.insert(
            "until".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalPlainDatePrototypeUntil.function_id(),
                false,
            )),
        );
        properties.insert(
            "since".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalPlainDatePrototypeSince.function_id(),
                false,
            )),
        );
        properties.insert(
            "toPlainDateTime".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalPlainDatePrototypeToPlainDateTime.function_id(),
                false,
            )),
        );
        properties.insert(
            "toZonedDateTime".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalPlainDatePrototypeToZonedDateTime.function_id(),
                false,
            )),
        );
        properties.insert(
            "toPlainYearMonth".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalPlainDatePrototypeToPlainYearMonth.function_id(),
                false,
            )),
        );
        properties.insert(
            "toPlainMonthDay".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalPlainDatePrototypeToPlainMonthDay.function_id(),
                false,
            )),
        );
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info("Temporal.PlainDate")),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::TemporalPlainDateConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn temporal_plain_date_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::temporal_plain_date_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    /// Temporal proposal 9.3: the `Temporal.PlainYearMonth.prototype` shape.
    /// The order mirrors `install_temporal_plain_year_month_constructor_intrinsics`,
    /// because property order is observable through `Object.keys`.
    pub(super) fn temporal_plain_year_month_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, getter) in [
            (
                "calendarId",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeCalendarIdGetter,
            ),
            (
                "era",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeEraGetter,
            ),
            (
                "eraYear",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeEraYearGetter,
            ),
            (
                "year",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeYearGetter,
            ),
            (
                "month",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthGetter,
            ),
            (
                "monthCode",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthCodeGetter,
            ),
            (
                "daysInYear",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInYearGetter,
            ),
            (
                "daysInMonth",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInMonthGetter,
            ),
            (
                "monthsInYear",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthsInYearGetter,
            ),
            (
                "inLeapYear",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeInLeapYearGetter,
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Accessor {
                    getter: Some(ObjectAccessorShape {
                        function_id: getter.function_id(),
                    }),
                    setter: None,
                },
            );
        }
        for (name, method) in [
            (
                "with",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeWith,
            ),
            ("add", StandardBuiltinId::TemporalPlainYearMonthPrototypeAdd),
            (
                "subtract",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeSubtract,
            ),
            (
                "until",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeUntil,
            ),
            (
                "since",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeSince,
            ),
            (
                "equals",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeEquals,
            ),
            (
                "toString",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeToString,
            ),
            (
                "toJSON",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeToJson,
            ),
            (
                "toLocaleString",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeToLocaleString,
            ),
            (
                "valueOf",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeValueOf,
            ),
            (
                "toPlainDate",
                StandardBuiltinId::TemporalPlainYearMonthPrototypeToPlainDate,
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    method.function_id(),
                    false,
                )),
            );
        }
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info("Temporal.PlainYearMonth")),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::TemporalPlainYearMonthConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn temporal_plain_year_month_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::temporal_plain_year_month_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    /// Temporal proposal 10.3: the `Temporal.PlainMonthDay.prototype` shape.
    pub(super) fn temporal_plain_month_day_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, getter) in [
            (
                "calendarId",
                StandardBuiltinId::TemporalPlainMonthDayPrototypeCalendarIdGetter,
            ),
            (
                "monthCode",
                StandardBuiltinId::TemporalPlainMonthDayPrototypeMonthCodeGetter,
            ),
            (
                "day",
                StandardBuiltinId::TemporalPlainMonthDayPrototypeDayGetter,
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Accessor {
                    getter: Some(ObjectAccessorShape {
                        function_id: getter.function_id(),
                    }),
                    setter: None,
                },
            );
        }
        for (name, method) in [
            (
                "with",
                StandardBuiltinId::TemporalPlainMonthDayPrototypeWith,
            ),
            (
                "equals",
                StandardBuiltinId::TemporalPlainMonthDayPrototypeEquals,
            ),
            (
                "toString",
                StandardBuiltinId::TemporalPlainMonthDayPrototypeToString,
            ),
            (
                "toJSON",
                StandardBuiltinId::TemporalPlainMonthDayPrototypeToJson,
            ),
            (
                "toLocaleString",
                StandardBuiltinId::TemporalPlainMonthDayPrototypeToLocaleString,
            ),
            (
                "valueOf",
                StandardBuiltinId::TemporalPlainMonthDayPrototypeValueOf,
            ),
            (
                "toPlainDate",
                StandardBuiltinId::TemporalPlainMonthDayPrototypeToPlainDate,
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    method.function_id(),
                    false,
                )),
            );
        }
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info("Temporal.PlainMonthDay")),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::TemporalPlainMonthDayConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn temporal_plain_month_day_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::temporal_plain_month_day_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    /// Temporal proposal 4.3: the `Temporal.PlainTime.prototype` shape. The
    /// order here mirrors `install_temporal_plain_time_constructor_intrinsics`,
    /// because property order is observable through `Object.keys`.
    pub(super) fn temporal_plain_time_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, getter) in [
            (
                "hour",
                StandardBuiltinId::TemporalPlainTimePrototypeHourGetter,
            ),
            (
                "minute",
                StandardBuiltinId::TemporalPlainTimePrototypeMinuteGetter,
            ),
            (
                "second",
                StandardBuiltinId::TemporalPlainTimePrototypeSecondGetter,
            ),
            (
                "millisecond",
                StandardBuiltinId::TemporalPlainTimePrototypeMillisecondGetter,
            ),
            (
                "microsecond",
                StandardBuiltinId::TemporalPlainTimePrototypeMicrosecondGetter,
            ),
            (
                "nanosecond",
                StandardBuiltinId::TemporalPlainTimePrototypeNanosecondGetter,
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Accessor {
                    getter: Some(ObjectAccessorShape {
                        function_id: getter.function_id(),
                    }),
                    setter: None,
                },
            );
        }
        for (name, builtin) in [
            ("add", StandardBuiltinId::TemporalPlainTimePrototypeAdd),
            (
                "subtract",
                StandardBuiltinId::TemporalPlainTimePrototypeSubtract,
            ),
            ("with", StandardBuiltinId::TemporalPlainTimePrototypeWith),
            ("until", StandardBuiltinId::TemporalPlainTimePrototypeUntil),
            ("since", StandardBuiltinId::TemporalPlainTimePrototypeSince),
            ("round", StandardBuiltinId::TemporalPlainTimePrototypeRound),
            (
                "equals",
                StandardBuiltinId::TemporalPlainTimePrototypeEquals,
            ),
            (
                "toString",
                StandardBuiltinId::TemporalPlainTimePrototypeToString,
            ),
            (
                "toJSON",
                StandardBuiltinId::TemporalPlainTimePrototypeToJson,
            ),
            (
                "toLocaleString",
                StandardBuiltinId::TemporalPlainTimePrototypeToLocaleString,
            ),
            (
                "valueOf",
                StandardBuiltinId::TemporalPlainTimePrototypeValueOf,
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    builtin.function_id(),
                    false,
                )),
            );
        }
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info("Temporal.PlainTime")),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::TemporalPlainTimeConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn temporal_plain_time_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::temporal_plain_time_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    /// Temporal proposal 5.3: the `Temporal.PlainDateTime.prototype` shape. The
    /// order here mirrors `install_temporal_plain_date_time_constructor_intrinsics`,
    /// because property order is observable through `Object.keys`.
    pub(super) fn temporal_plain_date_time_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, getter) in [
            (
                "calendarId",
                StandardBuiltinId::TemporalPlainDateTimePrototypeCalendarIdGetter,
            ),
            (
                "era",
                StandardBuiltinId::TemporalPlainDateTimePrototypeEraGetter,
            ),
            (
                "eraYear",
                StandardBuiltinId::TemporalPlainDateTimePrototypeEraYearGetter,
            ),
            (
                "year",
                StandardBuiltinId::TemporalPlainDateTimePrototypeYearGetter,
            ),
            (
                "month",
                StandardBuiltinId::TemporalPlainDateTimePrototypeMonthGetter,
            ),
            (
                "monthCode",
                StandardBuiltinId::TemporalPlainDateTimePrototypeMonthCodeGetter,
            ),
            (
                "day",
                StandardBuiltinId::TemporalPlainDateTimePrototypeDayGetter,
            ),
            (
                "hour",
                StandardBuiltinId::TemporalPlainDateTimePrototypeHourGetter,
            ),
            (
                "minute",
                StandardBuiltinId::TemporalPlainDateTimePrototypeMinuteGetter,
            ),
            (
                "second",
                StandardBuiltinId::TemporalPlainDateTimePrototypeSecondGetter,
            ),
            (
                "millisecond",
                StandardBuiltinId::TemporalPlainDateTimePrototypeMillisecondGetter,
            ),
            (
                "microsecond",
                StandardBuiltinId::TemporalPlainDateTimePrototypeMicrosecondGetter,
            ),
            (
                "nanosecond",
                StandardBuiltinId::TemporalPlainDateTimePrototypeNanosecondGetter,
            ),
            (
                "dayOfWeek",
                StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfWeekGetter,
            ),
            (
                "dayOfYear",
                StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfYearGetter,
            ),
            (
                "weekOfYear",
                StandardBuiltinId::TemporalPlainDateTimePrototypeWeekOfYearGetter,
            ),
            (
                "yearOfWeek",
                StandardBuiltinId::TemporalPlainDateTimePrototypeYearOfWeekGetter,
            ),
            (
                "daysInWeek",
                StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInWeekGetter,
            ),
            (
                "daysInMonth",
                StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInMonthGetter,
            ),
            (
                "daysInYear",
                StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInYearGetter,
            ),
            (
                "monthsInYear",
                StandardBuiltinId::TemporalPlainDateTimePrototypeMonthsInYearGetter,
            ),
            (
                "inLeapYear",
                StandardBuiltinId::TemporalPlainDateTimePrototypeInLeapYearGetter,
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Accessor {
                    getter: Some(ObjectAccessorShape {
                        function_id: getter.function_id(),
                    }),
                    setter: None,
                },
            );
        }
        for (name, builtin) in [
            (
                "with",
                StandardBuiltinId::TemporalPlainDateTimePrototypeWith,
            ),
            (
                "withPlainTime",
                StandardBuiltinId::TemporalPlainDateTimePrototypeWithPlainTime,
            ),
            (
                "withCalendar",
                StandardBuiltinId::TemporalPlainDateTimePrototypeWithCalendar,
            ),
            ("add", StandardBuiltinId::TemporalPlainDateTimePrototypeAdd),
            (
                "subtract",
                StandardBuiltinId::TemporalPlainDateTimePrototypeSubtract,
            ),
            (
                "until",
                StandardBuiltinId::TemporalPlainDateTimePrototypeUntil,
            ),
            (
                "since",
                StandardBuiltinId::TemporalPlainDateTimePrototypeSince,
            ),
            (
                "round",
                StandardBuiltinId::TemporalPlainDateTimePrototypeRound,
            ),
            (
                "equals",
                StandardBuiltinId::TemporalPlainDateTimePrototypeEquals,
            ),
            (
                "toString",
                StandardBuiltinId::TemporalPlainDateTimePrototypeToString,
            ),
            (
                "toJSON",
                StandardBuiltinId::TemporalPlainDateTimePrototypeToJson,
            ),
            (
                "toLocaleString",
                StandardBuiltinId::TemporalPlainDateTimePrototypeToLocaleString,
            ),
            (
                "valueOf",
                StandardBuiltinId::TemporalPlainDateTimePrototypeValueOf,
            ),
            (
                "toPlainDate",
                StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainDate,
            ),
            (
                "toPlainTime",
                StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainTime,
            ),
            (
                "toZonedDateTime",
                StandardBuiltinId::TemporalPlainDateTimePrototypeToZonedDateTime,
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    builtin.function_id(),
                    false,
                )),
            );
        }
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info("Temporal.PlainDateTime")),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::TemporalPlainDateTimeConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn temporal_plain_date_time_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::temporal_plain_date_time_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    /// Temporal proposal 7.3: the `Temporal.Duration.prototype` shape. The
    /// order here mirrors `install_temporal_duration_constructor_intrinsics`,
    /// because property order is observable through `Object.keys`.
    pub(super) fn temporal_duration_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, getter) in [
            (
                "years",
                StandardBuiltinId::TemporalDurationPrototypeYearsGetter,
            ),
            (
                "months",
                StandardBuiltinId::TemporalDurationPrototypeMonthsGetter,
            ),
            (
                "weeks",
                StandardBuiltinId::TemporalDurationPrototypeWeeksGetter,
            ),
            (
                "days",
                StandardBuiltinId::TemporalDurationPrototypeDaysGetter,
            ),
            (
                "hours",
                StandardBuiltinId::TemporalDurationPrototypeHoursGetter,
            ),
            (
                "minutes",
                StandardBuiltinId::TemporalDurationPrototypeMinutesGetter,
            ),
            (
                "seconds",
                StandardBuiltinId::TemporalDurationPrototypeSecondsGetter,
            ),
            (
                "milliseconds",
                StandardBuiltinId::TemporalDurationPrototypeMillisecondsGetter,
            ),
            (
                "microseconds",
                StandardBuiltinId::TemporalDurationPrototypeMicrosecondsGetter,
            ),
            (
                "nanoseconds",
                StandardBuiltinId::TemporalDurationPrototypeNanosecondsGetter,
            ),
            (
                "sign",
                StandardBuiltinId::TemporalDurationPrototypeSignGetter,
            ),
            (
                "blank",
                StandardBuiltinId::TemporalDurationPrototypeBlankGetter,
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Accessor {
                    getter: Some(ObjectAccessorShape {
                        function_id: getter.function_id(),
                    }),
                    setter: None,
                },
            );
        }
        properties.insert(
            "with".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalDurationPrototypeWith.function_id(),
                false,
            )),
        );
        properties.insert(
            "negated".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalDurationPrototypeNegated.function_id(),
                false,
            )),
        );
        properties.insert(
            "abs".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalDurationPrototypeAbs.function_id(),
                false,
            )),
        );
        properties.insert(
            "add".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalDurationPrototypeAdd.function_id(),
                false,
            )),
        );
        properties.insert(
            "subtract".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalDurationPrototypeSubtract.function_id(),
                false,
            )),
        );
        properties.insert(
            "round".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalDurationPrototypeRound.function_id(),
                false,
            )),
        );
        properties.insert(
            "total".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalDurationPrototypeTotal.function_id(),
                false,
            )),
        );
        properties.insert(
            "toString".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalDurationPrototypeToString.function_id(),
                false,
            )),
        );
        properties.insert(
            "toJSON".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalDurationPrototypeToJson.function_id(),
                false,
            )),
        );
        properties.insert(
            "toLocaleString".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalDurationPrototypeToLocaleString.function_id(),
                false,
            )),
        );
        properties.insert(
            "valueOf".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TemporalDurationPrototypeValueOf.function_id(),
                false,
            )),
        );
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info("Temporal.Duration")),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::TemporalDurationConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn temporal_duration_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::temporal_duration_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn regexp_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::regexp_prototype_shape()),
            properties: BTreeMap::from([(
                "lastIndex".to_string(),
                ObjectShapeProperty::Data(ValueInfo::new(ValueKind::Number)),
            )]),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn regexp_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        properties.insert(
            "flags".to_string(),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::RegExpPrototypeFlagsGetter.function_id(),
                }),
                setter: None,
            },
        );
        for (name, builtin) in [
            ("source", StandardBuiltinId::RegExpPrototypeSourceGetter),
            (
                "hasIndices",
                StandardBuiltinId::RegExpPrototypeHasIndicesGetter,
            ),
            ("global", StandardBuiltinId::RegExpPrototypeGlobalGetter),
            (
                "ignoreCase",
                StandardBuiltinId::RegExpPrototypeIgnoreCaseGetter,
            ),
            (
                "multiline",
                StandardBuiltinId::RegExpPrototypeMultilineGetter,
            ),
            ("dotAll", StandardBuiltinId::RegExpPrototypeDotAllGetter),
            ("unicode", StandardBuiltinId::RegExpPrototypeUnicodeGetter),
            (
                "unicodeSets",
                StandardBuiltinId::RegExpPrototypeUnicodeSetsGetter,
            ),
            ("sticky", StandardBuiltinId::RegExpPrototypeStickyGetter),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Accessor {
                    getter: Some(ObjectAccessorShape {
                        function_id: builtin.function_id(),
                    }),
                    setter: None,
                },
            );
        }
        properties.insert(
            "compile".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::RegExpPrototypeCompile.function_id(),
                false,
            )),
        );
        properties.insert(
            "exec".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::RegExpPrototypeExec.function_id(),
                false,
            )),
        );
        properties.insert(
            "test".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::RegExpPrototypeTest.function_id(),
                false,
            )),
        );
        properties.insert(
            "toString".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::RegExpPrototypeToString.function_id(),
                false,
            )),
        );
        properties.insert(
            shape_namespace_key(WellKnownSymbol::Match),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::RegExpPrototypeSymbolMatch.function_id(),
                false,
            )),
        );
        properties.insert(
            shape_namespace_key(WellKnownSymbol::MatchAll),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::RegExpPrototypeSymbolMatchAll.function_id(),
                false,
            )),
        );
        properties.insert(
            shape_namespace_key(WellKnownSymbol::Replace),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::RegExpPrototypeSymbolReplace.function_id(),
                false,
            )),
        );
        properties.insert(
            shape_namespace_key(WellKnownSymbol::Search),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::RegExpPrototypeSymbolSearch.function_id(),
                false,
            )),
        );
        properties.insert(
            shape_namespace_key(WellKnownSymbol::Split),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::RegExpPrototypeSymbolSplit.function_id(),
                false,
            )),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::RegExpConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn typed_array_instance_shape() -> Box<HeapShape> {
        Self::typed_array_instance_shape_with_prototype(Self::typed_array_prototype_shape())
    }

    pub(super) fn uint8_array_codec_result_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties: ["read", "written"]
                .into_iter()
                .map(|name| {
                    (
                        name.to_string(),
                        ObjectShapeProperty::Data(ValueInfo::new(ValueKind::Number)),
                    )
                })
                .collect(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn typed_array_instance_shape_for_constructor(
        builtin: StandardBuiltinId,
    ) -> Box<HeapShape> {
        Self::typed_array_instance_shape_with_prototype(
            Self::typed_array_constructor_prototype_shape(builtin),
        )
    }

    pub(super) fn typed_array_instance_shape_with_prototype(
        prototype: Box<HeapShape>,
    ) -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(prototype),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn typed_array_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        properties.insert(
            "constructor".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::TypedArrayConstructor.function_id(),
                true,
            )),
        );
        for (name, function_id) in [
            (
                "buffer",
                StandardBuiltinId::TypedArrayPrototypeBufferGetter.function_id(),
            ),
            (
                "byteLength",
                StandardBuiltinId::TypedArrayPrototypeByteLengthGetter.function_id(),
            ),
            (
                "byteOffset",
                StandardBuiltinId::TypedArrayPrototypeByteOffsetGetter.function_id(),
            ),
            (
                "length",
                StandardBuiltinId::TypedArrayPrototypeLengthGetter.function_id(),
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Accessor {
                    getter: Some(ObjectAccessorShape { function_id }),
                    setter: None,
                },
            );
        }
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::TypedArrayPrototypeToStringTagGetter
                        .function_id(),
                }),
                setter: None,
            },
        );
        properties.insert(
            "at".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::TypedArrayPrototypeAt,
            )),
        );
        for (name, builtin) in [
            ("includes", StandardBuiltinId::TypedArrayPrototypeIncludes),
            ("indexOf", StandardBuiltinId::TypedArrayPrototypeIndexOf),
            (
                "lastIndexOf",
                StandardBuiltinId::TypedArrayPrototypeLastIndexOf,
            ),
            ("find", StandardBuiltinId::TypedArrayPrototypeFind),
            ("findIndex", StandardBuiltinId::TypedArrayPrototypeFindIndex),
            ("findLast", StandardBuiltinId::TypedArrayPrototypeFindLast),
            (
                "findLastIndex",
                StandardBuiltinId::TypedArrayPrototypeFindLastIndex,
            ),
            ("every", StandardBuiltinId::TypedArrayPrototypeEvery),
            ("some", StandardBuiltinId::TypedArrayPrototypeSome),
            ("map", StandardBuiltinId::TypedArrayPrototypeMap),
            ("filter", StandardBuiltinId::TypedArrayPrototypeFilter),
            ("forEach", StandardBuiltinId::TypedArrayPrototypeForEach),
            ("reduce", StandardBuiltinId::TypedArrayPrototypeReduce),
            (
                "reduceRight",
                StandardBuiltinId::TypedArrayPrototypeReduceRight,
            ),
            ("values", StandardBuiltinId::TypedArrayPrototypeValues),
            ("keys", StandardBuiltinId::TypedArrayPrototypeKeys),
            ("entries", StandardBuiltinId::TypedArrayPrototypeEntries),
            ("toString", StandardBuiltinId::TypedArrayPrototypeToString),
            ("fill", StandardBuiltinId::TypedArrayPrototypeFill),
            ("join", StandardBuiltinId::TypedArrayPrototypeJoin),
            ("set", StandardBuiltinId::TypedArrayPrototypeSet),
            ("reverse", StandardBuiltinId::TypedArrayPrototypeReverse),
            (
                "copyWithin",
                StandardBuiltinId::TypedArrayPrototypeCopyWithin,
            ),
            ("sort", StandardBuiltinId::TypedArrayPrototypeSort),
            (
                "toReversed",
                StandardBuiltinId::TypedArrayPrototypeToReversed,
            ),
            ("toSorted", StandardBuiltinId::TypedArrayPrototypeToSorted),
            ("with", StandardBuiltinId::TypedArrayPrototypeWith),
            ("subarray", StandardBuiltinId::TypedArrayPrototypeSubarray),
            ("slice", StandardBuiltinId::TypedArrayPrototypeSlice),
            (
                "toLocaleString",
                StandardBuiltinId::TypedArrayPrototypeToLocaleString,
            ),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::standard_builtin_value_info(builtin)),
            );
        }
        properties.insert(
            shape_namespace_key(WellKnownSymbol::Iterator),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::TypedArrayPrototypeValues,
            )),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::TypedArrayConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn typed_array_constructor_prototype_shape(
        builtin: StandardBuiltinId,
    ) -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        properties.insert(
            "constructor".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                builtin.function_id(),
                true,
            )),
        );
        if builtin == StandardBuiltinId::Uint8ArrayConstructor {
            for (name, method) in UINT8_ARRAY_CODEC_PROTOTYPE_MEMBERS {
                properties.insert(
                    name.to_string(),
                    ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                        method.function_id(),
                        false,
                    )),
                );
            }
        }
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(builtin),
            prototype: Some(Self::typed_array_prototype_shape()),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn array_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for builtin in [
            StandardBuiltinId::ArrayPrototypeConcat,
            StandardBuiltinId::ArrayPrototypeJoin,
            StandardBuiltinId::ArrayPrototypeSlice,
            StandardBuiltinId::ArrayPrototypeSplice,
            StandardBuiltinId::ArrayPrototypeSort,
            StandardBuiltinId::TypedArrayPrototypeToString,
            StandardBuiltinId::ArrayPrototypeToLocaleString,
            StandardBuiltinId::ArrayPrototypeFlat,
            StandardBuiltinId::ArrayPrototypeFlatMap,
            StandardBuiltinId::ArrayPrototypeAt,
            StandardBuiltinId::ArrayPrototypeToReversed,
            StandardBuiltinId::ArrayPrototypeToSpliced,
            StandardBuiltinId::ArrayPrototypeToSorted,
            StandardBuiltinId::ArrayPrototypeWith,
            StandardBuiltinId::ArrayPrototypeReverse,
            StandardBuiltinId::ArrayPrototypeCopyWithin,
            StandardBuiltinId::ArrayPrototypeIncludes,
            StandardBuiltinId::ArrayPrototypeIndexOf,
            StandardBuiltinId::ArrayPrototypeLastIndexOf,
            StandardBuiltinId::ArrayPrototypeFind,
            StandardBuiltinId::ArrayPrototypeFindIndex,
            StandardBuiltinId::ArrayPrototypeFindLast,
            StandardBuiltinId::ArrayPrototypeFindLastIndex,
            StandardBuiltinId::ArrayPrototypeEvery,
            StandardBuiltinId::ArrayPrototypeSome,
            StandardBuiltinId::ArrayPrototypeForEach,
            StandardBuiltinId::ArrayPrototypeFilter,
            StandardBuiltinId::ArrayPrototypeMap,
            StandardBuiltinId::ArrayPrototypeReduce,
            StandardBuiltinId::ArrayPrototypeReduceRight,
            StandardBuiltinId::ArrayPrototypePop,
            StandardBuiltinId::ArrayPrototypePush,
            StandardBuiltinId::ArrayPrototypeShift,
            StandardBuiltinId::ArrayPrototypeUnshift,
            StandardBuiltinId::ArrayPrototypeFill,
            StandardBuiltinId::ArrayPrototypeKeys,
            StandardBuiltinId::ArrayPrototypeEntries,
            StandardBuiltinId::ArrayPrototypeValues,
        ] {
            if let Some(name) = builtin.native_function_name() {
                properties.insert(
                    name.to_string(),
                    ObjectShapeProperty::Data(Self::standard_builtin_value_info(builtin)),
                );
            }
        }
        properties.insert(
            shape_namespace_key(WellKnownSymbol::Iterator),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::ArrayPrototypeValues,
            )),
        );
        Box::new(HeapShape::Array(ArrayShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::ArrayConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            elements: Vec::new(),
        }))
    }

    pub(super) fn is_typed_array_constructor(builtin: StandardBuiltinId) -> bool {
        matches!(
            builtin,
            StandardBuiltinId::Float64ArrayConstructor
                | StandardBuiltinId::Float32ArrayConstructor
                | StandardBuiltinId::Float16ArrayConstructor
                | StandardBuiltinId::Int32ArrayConstructor
                | StandardBuiltinId::Int16ArrayConstructor
                | StandardBuiltinId::Int8ArrayConstructor
                | StandardBuiltinId::Uint32ArrayConstructor
                | StandardBuiltinId::Uint16ArrayConstructor
                | StandardBuiltinId::Uint8ArrayConstructor
                | StandardBuiltinId::Uint8ClampedArrayConstructor
                | StandardBuiltinId::BigInt64ArrayConstructor
                | StandardBuiltinId::BigUint64ArrayConstructor
        )
    }

    pub(super) fn fresh_constructed_instance_with_private_brands(
        private_brands: BTreeSet<PrivateNameId>,
    ) -> ValueInfo {
        ValueInfo {
            kind: ValueKind::Object,
            possible_kinds: KindSet::from_kind(ValueKind::Object),
            heap_shape: Some(Box::new(HeapShape::Object(ObjectShape {
                provenance: HeapShapeProvenance::Program,
                prototype: None,
                properties: BTreeMap::new(),
                private_brands,
                boxed_primitive: None,
            }))),
            function_targets: FunctionTargetKnowledge::none(),
        }
    }

    pub(super) fn function_heap_shape(constructable: bool) -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        if constructable {
            properties.insert(
                "prototype".to_string(),
                ObjectShapeProperty::Data(Self::fresh_constructed_instance_info()),
            );
        }
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: None,
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    /// The intrinsic prototype carried by a generator/async function object.
    /// Its `constructor` data property is the semantic identity used by T13;
    /// the identity has no backend emitter and can only produce a typed
    /// dynamic-source diagnostic.
    pub(super) fn derived_function_intrinsic_prototype_shape(
        kind: DynamicFunctionKind,
    ) -> Box<HeapShape> {
        let intrinsic = DynamicSourceIntrinsic::Function(kind);
        debug_assert_ne!(kind, DynamicFunctionKind::Ordinary);
        let properties = BTreeMap::from([(
            "constructor".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                intrinsic.function_id().to_string(),
                intrinsic.constructable(),
            )),
        )]);
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::UntrackedIntrinsicPrototype,
            prototype: Some(Self::function_heap_shape(false)),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn function_value_info_with_intrinsic_prototype(
        &self,
        function_id: &FunctionId,
    ) -> ValueInfo {
        let constructable = self
            .function_signatures
            .get(function_id)
            .is_some_and(|signature| signature.protocol.is_constructable());
        let mut info =
            Self::function_value_info_with_constructable(function_id.clone(), constructable);
        let Some(kind) = self
            .function_signatures
            .get(function_id)
            .and_then(|signature| {
                DynamicFunctionKind::from_derived_execution_kind(
                    signature.protocol.execution_kind(),
                )
            })
        else {
            return info;
        };
        let Some(HeapShape::Object(shape)) = info.heap_shape.as_mut().map(Box::as_mut) else {
            return info;
        };
        shape.prototype = Some(Self::derived_function_intrinsic_prototype_shape(kind));
        info
    }

    pub(super) fn function_value_info_with_constructable(
        function_id: FunctionId,
        constructable: bool,
    ) -> ValueInfo {
        ValueInfo {
            kind: ValueKind::Function,
            possible_kinds: KindSet::from_kind(ValueKind::Function),
            heap_shape: Some(Self::function_heap_shape(constructable)),
            function_targets: FunctionTargetKnowledge::exact(function_id),
        }
    }

    pub(super) fn function_construct_this_info(function_id: FunctionId) -> ValueInfo {
        let function_info = Self::function_value_info_with_constructable(function_id, true);
        let prototype = match function_info.heap_shape.as_deref() {
            Some(HeapShape::Object(object)) => {
                object
                    .properties
                    .get("prototype")
                    .and_then(|property| match property {
                        ObjectShapeProperty::Data(info) => info.heap_shape.clone(),
                        ObjectShapeProperty::Accessor { .. } => None,
                    })
            }
            _ => None,
        };
        Self::with_instance_prototype(Self::fresh_constructed_instance_info(), prototype)
    }

    pub(super) fn host_function_value_info(builtin: HostBuiltinId) -> ValueInfo {
        Self::function_value_info_with_constructable(builtin.function_id(), false)
    }

    pub(super) fn string_value_info(value: &str) -> ValueInfo {
        let _ = value;
        ValueInfo {
            kind: ValueKind::String,
            possible_kinds: KindSet::from_kind(ValueKind::String),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::none(),
        }
    }

    pub(super) fn utf16_units_to_runtime_string(units: &[u16]) -> String {
        encode_js_string_utf16(units)
    }

    /// Semantic values preserve UTF-16 units; Interner Display is diagnostic
    /// text and escapes an unpaired surrogate as a different property key.
    pub(super) fn interned_runtime_string(&self, symbol: boa_interner::Sym) -> String {
        Self::utf16_units_to_runtime_string(self.interner.resolve_expect(symbol).utf16())
    }

    pub(super) fn error_message_value_info() -> ValueInfo {
        ValueInfo {
            kind: ValueKind::Dynamic,
            possible_kinds: KindSet::from_kind(ValueKind::Undefined)
                .union(KindSet::from_kind(ValueKind::String)),
            heap_shape: None,
            function_targets: FunctionTargetKnowledge::none(),
        }
    }

    pub(super) fn boxed_primitive_kind_for_value_kind(
        kind: ValueKind,
    ) -> Option<BoxedPrimitiveKind> {
        match kind {
            ValueKind::Number => Some(BoxedPrimitiveKind::Number),
            ValueKind::String => Some(BoxedPrimitiveKind::String),
            ValueKind::Boolean => Some(BoxedPrimitiveKind::Boolean),
            ValueKind::Symbol => Some(BoxedPrimitiveKind::Symbol),
            ValueKind::BigInt => Some(BoxedPrimitiveKind::BigInt),
            _ => None,
        }
    }

    pub(super) fn boxed_primitive_kind_set() -> KindSet {
        KindSet::from_kind(ValueKind::Boolean)
            .union(KindSet::from_kind(ValueKind::Number))
            .union(KindSet::from_kind(ValueKind::String))
            .union(KindSet::from_kind(ValueKind::Symbol))
            .union(KindSet::from_kind(ValueKind::BigInt))
    }

    pub(super) fn standard_boxed_prototype_shape(kind: BoxedPrimitiveKind) -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        if kind == BoxedPrimitiveKind::String {
            for builtin in [
                StandardBuiltinId::StringPrototypeToString,
                StandardBuiltinId::StringPrototypeValueOf,
                StandardBuiltinId::StringPrototypeCharAt,
                StandardBuiltinId::StringPrototypeConcat,
                StandardBuiltinId::StringPrototypeCharCodeAt,
                StandardBuiltinId::StringPrototypeCodePointAt,
                StandardBuiltinId::StringPrototypeAt,
                StandardBuiltinId::StringPrototypeAnchor,
                StandardBuiltinId::StringPrototypeBig,
                StandardBuiltinId::StringPrototypeBlink,
                StandardBuiltinId::StringPrototypeBold,
                StandardBuiltinId::StringPrototypeFixed,
                StandardBuiltinId::StringPrototypeFontcolor,
                StandardBuiltinId::StringPrototypeFontsize,
                StandardBuiltinId::StringPrototypeItalics,
                StandardBuiltinId::StringPrototypeLink,
                StandardBuiltinId::StringPrototypeSmall,
                StandardBuiltinId::StringPrototypeStrike,
                StandardBuiltinId::StringPrototypeSub,
                StandardBuiltinId::StringPrototypeSubstr,
                StandardBuiltinId::StringPrototypeSubstring,
                StandardBuiltinId::StringPrototypeSup,
                StandardBuiltinId::StringPrototypeIndexOf,
                StandardBuiltinId::StringPrototypeLastIndexOf,
                StandardBuiltinId::StringPrototypeSlice,
                StandardBuiltinId::StringPrototypeSplit,
                StandardBuiltinId::StringPrototypePadStart,
                StandardBuiltinId::StringPrototypePadEnd,
                StandardBuiltinId::StringPrototypeRepeat,
                StandardBuiltinId::StringPrototypeNormalize,
                StandardBuiltinId::StringPrototypeLocaleCompare,
                StandardBuiltinId::StringPrototypeToLocaleLowerCase,
                StandardBuiltinId::StringPrototypeToLocaleUpperCase,
                StandardBuiltinId::StringPrototypeToLowerCase,
                StandardBuiltinId::StringPrototypeToUpperCase,
                StandardBuiltinId::StringPrototypeTrim,
                StandardBuiltinId::StringPrototypeTrimStart,
                StandardBuiltinId::StringPrototypeTrimEnd,
                StandardBuiltinId::StringPrototypeIsWellFormed,
                StandardBuiltinId::StringPrototypeToWellFormed,
            ] {
                properties.insert(
                    builtin.string_prototype_method_name().unwrap().to_string(),
                    ObjectShapeProperty::Data(Self::standard_builtin_value_info(builtin)),
                );
            }
            properties.insert(
                shape_namespace_key(WellKnownSymbol::Iterator),
                ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                    StandardBuiltinId::StringPrototypeIterator,
                )),
            );
        } else if kind == BoxedPrimitiveKind::Number {
            for builtin in [
                StandardBuiltinId::NumberPrototypeToExponential,
                StandardBuiltinId::NumberPrototypeToFixed,
                StandardBuiltinId::NumberPrototypeToPrecision,
                StandardBuiltinId::NumberPrototypeToString,
                StandardBuiltinId::NumberPrototypeToLocaleString,
                StandardBuiltinId::NumberPrototypeValueOf,
            ] {
                properties.insert(
                    builtin.native_function_name().unwrap().to_string(),
                    ObjectShapeProperty::Data(Self::standard_builtin_value_info(builtin)),
                );
            }
        } else if kind == BoxedPrimitiveKind::Boolean {
            for builtin in [
                StandardBuiltinId::BooleanPrototypeToString,
                StandardBuiltinId::BooleanPrototypeValueOf,
            ] {
                properties.insert(
                    builtin.native_function_name().unwrap().to_string(),
                    ObjectShapeProperty::Data(Self::standard_builtin_value_info(builtin)),
                );
            }
        } else if kind == BoxedPrimitiveKind::BigInt {
            for builtin in [
                StandardBuiltinId::BigIntPrototypeToString,
                StandardBuiltinId::BigIntPrototypeToLocaleString,
                StandardBuiltinId::BigIntPrototypeValueOf,
            ] {
                properties.insert(
                    builtin.native_function_name().unwrap().to_string(),
                    ObjectShapeProperty::Data(Self::standard_builtin_value_info(builtin)),
                );
            }
        } else if kind == BoxedPrimitiveKind::Symbol {
            for builtin in [
                StandardBuiltinId::SymbolPrototypeToString,
                StandardBuiltinId::SymbolPrototypeValueOf,
            ] {
                properties.insert(
                    builtin.native_function_name().unwrap().to_string(),
                    ObjectShapeProperty::Data(Self::standard_builtin_value_info(builtin)),
                );
            }
            properties.insert(
                shape_namespace_key(WellKnownSymbol::ToPrimitive),
                ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                    StandardBuiltinId::SymbolPrototypeToPrimitive,
                )),
            );
            properties.insert(
                "description".to_string(),
                ObjectShapeProperty::Accessor {
                    getter: Some(ObjectAccessorShape {
                        function_id: StandardBuiltinId::SymbolPrototypeDescriptionGetter
                            .function_id(),
                    }),
                    setter: None,
                },
            );
        }
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(match kind {
                BoxedPrimitiveKind::Number => StandardBuiltinId::NumberConstructor,
                BoxedPrimitiveKind::String => StandardBuiltinId::StringConstructor,
                BoxedPrimitiveKind::Boolean => StandardBuiltinId::BooleanConstructor,
                BoxedPrimitiveKind::Symbol => StandardBuiltinId::SymbolConstructor,
                BoxedPrimitiveKind::BigInt => StandardBuiltinId::BigIntConstructor,
            }),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn boxed_primitive_instance_info(primitive: ValueInfo) -> ValueInfo {
        let mut properties = BTreeMap::new();
        if primitive.possible_kinds == KindSet::from_kind(ValueKind::String) {
            properties.insert(
                "length".to_string(),
                ObjectShapeProperty::Data(ValueInfo::new(ValueKind::Number)),
            );
        }
        ValueInfo {
            kind: ValueKind::Object,
            possible_kinds: KindSet::from_kind(ValueKind::Object),
            heap_shape: Some(Box::new(HeapShape::Object(ObjectShape {
                provenance: HeapShapeProvenance::Program,
                prototype: Self::boxed_primitive_kind_for_value_kind(primitive.kind)
                    .map(Self::standard_boxed_prototype_shape),
                properties,
                private_brands: BTreeSet::new(),
                boxed_primitive: Some(Box::new(primitive)),
            }))),
            function_targets: FunctionTargetKnowledge::none(),
        }
    }

    pub(super) fn value_info_is_boxed_string(info: &ValueInfo) -> bool {
        matches!(
            info.heap_shape.as_deref(),
            Some(HeapShape::Object(ObjectShape {
                boxed_primitive: Some(primitive),
                ..
            })) if primitive.possible_kinds.contains(ValueKind::String)
        )
    }

    pub(super) fn standard_error_prototype_shape(builtin: StandardBuiltinId) -> Box<HeapShape> {
        let prototype = match builtin {
            StandardBuiltinId::ErrorConstructor => Some(Box::new(Self::empty_object_shape())),
            StandardBuiltinId::EvalErrorConstructor
            | StandardBuiltinId::AggregateErrorConstructor
            | StandardBuiltinId::SuppressedErrorConstructor
            | StandardBuiltinId::RangeErrorConstructor
            | StandardBuiltinId::SyntaxErrorConstructor
            | StandardBuiltinId::TypeErrorConstructor
            | StandardBuiltinId::URIErrorConstructor
            | StandardBuiltinId::ReferenceErrorConstructor => Some(
                Self::standard_error_prototype_shape(StandardBuiltinId::ErrorConstructor),
            ),
            _ => Some(Box::new(Self::empty_object_shape())),
        };
        let mut properties = BTreeMap::new();
        properties.insert(
            "name".to_string(),
            ObjectShapeProperty::Data(Self::string_value_info(builtin.debug_name())),
        );
        // 20.5.3.4 and 20.5.6.3: only `%Error.prototype%` owns `toString`.
        // An own copy on every NativeError prototype hid a replaced
        // `Error.prototype.toString` from every TypeError, RangeError, ...
        if builtin == StandardBuiltinId::ErrorConstructor {
            properties.insert(
                "toString".to_string(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    StandardBuiltinId::ErrorPrototypeToString.function_id(),
                    false,
                )),
            );
            properties.insert(
                "stack".to_string(),
                ObjectShapeProperty::Accessor {
                    getter: Some(ObjectAccessorShape {
                        function_id: StandardBuiltinId::ErrorPrototypeStackGetter.function_id(),
                    }),
                    setter: Some(ObjectAccessorShape {
                        function_id: StandardBuiltinId::ErrorPrototypeStackSetter.function_id(),
                    }),
                },
            );
        }
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(builtin),
            prototype,
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    /// A fresh object of the intrinsic error constructor `builtin`.
    ///
    /// Its `[[Prototype]]` is the intrinsic `%NativeError.prototype%`, whose
    /// state here is the recorded global fact rather than the fresh-realm
    /// catalogue: after `Error.prototype.toString = f` a new error must reach
    /// `f`, not the builtin. Without that fact — or when an aliasing write cut
    /// the recorded link from a NativeError prototype to `%Error.prototype%` —
    /// the prototype chain is unknown. A shape whose `prototype` is `None`
    /// reads as a plain object inheriting `%Object.prototype%`, so the object
    /// is left unshaped instead.
    pub(super) fn standard_error_instance_info(&self, builtin: StandardBuiltinId) -> ValueInfo {
        let prototype = self
            .live_intrinsic_prototype(builtin)
            .and_then(|prototype| prototype.heap_shape.clone())
            .filter(|prototype| match prototype.as_ref() {
                HeapShape::Object(prototype) => {
                    builtin == StandardBuiltinId::ErrorConstructor || prototype.prototype.is_some()
                }
                HeapShape::Array(_) => false,
            });
        let Some(prototype) = prototype else {
            return ValueInfo {
                kind: ValueKind::Object,
                possible_kinds: KindSet::from_kind(ValueKind::Object),
                heap_shape: None,
                function_targets: FunctionTargetKnowledge::none(),
            };
        };
        let mut properties = BTreeMap::new();
        properties.insert(
            "message".to_string(),
            ObjectShapeProperty::Data(Self::error_message_value_info()),
        );
        if builtin == StandardBuiltinId::AggregateErrorConstructor {
            properties.insert(
                "errors".to_string(),
                ObjectShapeProperty::Data(ValueInfo {
                    kind: ValueKind::Array,
                    possible_kinds: KindSet::from_kind(ValueKind::Array),
                    heap_shape: Some(Box::new(HeapShape::Array(ArrayShape::default()))),
                    function_targets: FunctionTargetKnowledge::none(),
                }),
            );
        }
        if builtin == StandardBuiltinId::SuppressedErrorConstructor {
            properties.insert(
                "error".to_string(),
                ObjectShapeProperty::Data(ValueInfo::new(ValueKind::Dynamic)),
            );
            properties.insert(
                "suppressed".to_string(),
                ObjectShapeProperty::Data(ValueInfo::new(ValueKind::Dynamic)),
            );
        }
        ValueInfo {
            kind: ValueKind::Object,
            possible_kinds: KindSet::from_kind(ValueKind::Object),
            heap_shape: Some(Box::new(HeapShape::Object(ObjectShape {
                provenance: HeapShapeProvenance::Program,
                prototype: Some(prototype),
                properties,
                private_brands: BTreeSet::new(),
                boxed_primitive: None,
            }))),
            function_targets: FunctionTargetKnowledge::none(),
        }
    }

    pub(super) fn standard_builtin_function_shape(builtin: StandardBuiltinId) -> Box<HeapShape> {
        let mut shape = Self::function_heap_shape(builtin.constructable());
        if let HeapShape::Object(object) = shape.as_mut() {
            match builtin {
                StandardBuiltinId::ShadowRealmConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::shadow_realm_prototype_shape(),
                        ))),
                    );
                }
                StandardBuiltinId::FunctionConstructor => {
                    // `%Function.prototype%` is itself the exact callable
                    // intrinsic. Keeping its catalog target in the constructor
                    // shape makes `Function.prototype()` a statically resolved
                    // call instead of an indirect call through an Object-shaped
                    // placeholder.
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                            StandardBuiltinId::FunctionPrototype,
                        )),
                    );
                }
                StandardBuiltinId::FunctionPrototype => {
                    object.provenance = HeapShapeProvenance::IntrinsicPrototype(
                        StandardBuiltinId::FunctionConstructor,
                    );
                    // This non-configurable symbol property is part of the exact
                    // callable intrinsic shape. A computed read through
                    // `Function.prototype[Symbol.hasInstance]` can therefore
                    // retain the catalogued call target without treating symbol
                    // and string property keys as interchangeable.
                    object.properties.insert(
                        shape_namespace_key(WellKnownSymbol::HasInstance),
                        ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                            StandardBuiltinId::FunctionPrototypeSymbolHasInstance,
                        )),
                    );
                }
                StandardBuiltinId::TypedArrayConstructor => {
                    object.properties.insert(
                        shape_namespace_key(WellKnownSymbol::Species),
                        ObjectShapeProperty::Accessor {
                            getter: Some(ObjectAccessorShape {
                                function_id: StandardBuiltinId::TypedArraySpeciesGetter
                                    .function_id(),
                            }),
                            setter: None,
                        },
                    );
                    object.properties.insert(
                        "from".to_string(),
                        ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                            StandardBuiltinId::TypedArrayFrom,
                        )),
                    );
                    object.properties.insert(
                        "of".to_string(),
                        ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                            StandardBuiltinId::TypedArrayOf,
                        )),
                    );
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::typed_array_prototype_shape(),
                        ))),
                    );
                }
                StandardBuiltinId::PromiseConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::promise_prototype_shape(),
                        ))),
                    );
                    object.properties.insert(
                        "resolve".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::PromiseResolve.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "withResolvers".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::PromiseWithResolvers.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "try".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::PromiseTry.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "reject".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::PromiseReject.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "all".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::PromiseAll.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "allSettled".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::PromiseAllSettled.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "allKeyed".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::PromiseAllKeyed.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "allSettledKeyed".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::PromiseAllSettledKeyed.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "any".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::PromiseAny.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "race".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::PromiseRace.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        shape_namespace_key(WellKnownSymbol::Species),
                        ObjectShapeProperty::Accessor {
                            getter: Some(ObjectAccessorShape {
                                function_id: StandardBuiltinId::PromiseSpeciesGetter.function_id(),
                            }),
                            setter: None,
                        },
                    );
                }
                StandardBuiltinId::MapConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::map_prototype_shape(),
                        ))),
                    );
                    object.properties.insert(
                        "groupBy".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::MapGroupBy.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        shape_namespace_key(WellKnownSymbol::Species),
                        ObjectShapeProperty::Accessor {
                            getter: Some(ObjectAccessorShape {
                                function_id: StandardBuiltinId::MapSpeciesGetter.function_id(),
                            }),
                            setter: None,
                        },
                    );
                }
                StandardBuiltinId::WeakMapConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::weak_map_prototype_shape(),
                        ))),
                    );
                }
                StandardBuiltinId::WeakSetConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::weak_set_prototype_shape(),
                        ))),
                    );
                }
                StandardBuiltinId::WeakRefConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::weak_ref_prototype_shape(),
                        ))),
                    );
                }
                StandardBuiltinId::FinalizationRegistryConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::finalization_registry_prototype_shape(),
                        ))),
                    );
                }
                StandardBuiltinId::SetConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::set_prototype_shape(),
                        ))),
                    );
                    object.properties.insert(
                        shape_namespace_key(WellKnownSymbol::Species),
                        ObjectShapeProperty::Accessor {
                            getter: Some(ObjectAccessorShape {
                                function_id: StandardBuiltinId::SetSpeciesGetter.function_id(),
                            }),
                            setter: None,
                        },
                    );
                }
                StandardBuiltinId::NumberConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(ValueInfo {
                            kind: ValueKind::Object,
                            possible_kinds: KindSet::from_kind(ValueKind::Object),
                            heap_shape: Some(Self::standard_boxed_prototype_shape(
                                BoxedPrimitiveKind::Number,
                            )),
                            function_targets: FunctionTargetKnowledge::none(),
                        }),
                    );
                    object.properties.insert(
                        "isInteger".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::NumberIsInteger.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "isSafeInteger".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::NumberIsSafeInteger.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "isFinite".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::NumberIsFinite.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "isNaN".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::NumberIsNaN.function_id(),
                            false,
                        )),
                    );
                    for name in [
                        "NaN",
                        "POSITIVE_INFINITY",
                        "NEGATIVE_INFINITY",
                        "MAX_VALUE",
                        "MIN_VALUE",
                        "EPSILON",
                        "MAX_SAFE_INTEGER",
                        "MIN_SAFE_INTEGER",
                    ] {
                        object.properties.insert(
                            name.to_string(),
                            ObjectShapeProperty::Data(ValueInfo::new(ValueKind::Number)),
                        );
                    }
                }
                StandardBuiltinId::BigIntConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(ValueInfo {
                            kind: ValueKind::Object,
                            possible_kinds: KindSet::from_kind(ValueKind::Object),
                            heap_shape: Some(Self::standard_boxed_prototype_shape(
                                BoxedPrimitiveKind::BigInt,
                            )),
                            function_targets: FunctionTargetKnowledge::none(),
                        }),
                    );
                    object.properties.insert(
                        "asIntN".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::BigIntAsIntN.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "asUintN".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::BigIntAsUintN.function_id(),
                            false,
                        )),
                    );
                }
                StandardBuiltinId::StringConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(ValueInfo {
                            kind: ValueKind::Object,
                            possible_kinds: KindSet::from_kind(ValueKind::Object),
                            heap_shape: Some(Self::standard_boxed_prototype_shape(
                                BoxedPrimitiveKind::String,
                            )),
                            function_targets: FunctionTargetKnowledge::none(),
                        }),
                    );
                    object.properties.insert(
                        "fromCodePoint".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::StringFromCodePoint.function_id(),
                            false,
                        )),
                    );
                }
                StandardBuiltinId::BooleanConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(ValueInfo {
                            kind: ValueKind::Object,
                            possible_kinds: KindSet::from_kind(ValueKind::Object),
                            heap_shape: Some(Self::standard_boxed_prototype_shape(
                                BoxedPrimitiveKind::Boolean,
                            )),
                            function_targets: FunctionTargetKnowledge::none(),
                        }),
                    );
                }
                StandardBuiltinId::SymbolConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(ValueInfo {
                            kind: ValueKind::Object,
                            possible_kinds: KindSet::from_kind(ValueKind::Object),
                            heap_shape: Some(Self::standard_boxed_prototype_shape(
                                BoxedPrimitiveKind::Symbol,
                            )),
                            function_targets: FunctionTargetKnowledge::none(),
                        }),
                    );
                    object.properties.insert(
                        "for".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::SymbolFor.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "keyFor".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::SymbolKeyFor.function_id(),
                            false,
                        )),
                    );
                }
                StandardBuiltinId::ObjectConstructor => {
                    object.properties.insert(
                        "groupBy".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectGroupBy.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "fromEntries".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectFromEntries.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "assign".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectAssign.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "create".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectCreate.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "getPrototypeOf".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectGetPrototypeOf.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "setPrototypeOf".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectSetPrototypeOf.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "defineProperty".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectDefineProperty.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "defineProperties".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectDefineProperties.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "getOwnPropertyDescriptor".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectGetOwnPropertyDescriptor.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "getOwnPropertyDescriptors".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectGetOwnPropertyDescriptors.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "getOwnPropertyNames".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectGetOwnPropertyNames.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "getOwnPropertySymbols".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectGetOwnPropertySymbols.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "keys".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectKeys.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "values".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectValues.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "entries".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectEntries.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "hasOwn".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectHasOwn.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "is".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectIs.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "isSealed".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectIsSealed.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "isFrozen".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectIsFrozen.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "seal".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectSeal.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "freeze".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectFreeze.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "isExtensible".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectIsExtensible.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "preventExtensions".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ObjectPreventExtensions.function_id(),
                            false,
                        )),
                    );
                }
                StandardBuiltinId::IteratorConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::iterator_prototype_shape(),
                        ))),
                    );
                    object.properties.insert(
                        "from".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::IteratorFrom.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "concat".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::IteratorConcat.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "zip".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::IteratorZip.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "zipKeyed".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::IteratorZipKeyed.function_id(),
                            false,
                        )),
                    );
                }
                StandardBuiltinId::ArrayConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(ValueInfo {
                            kind: ValueKind::Array,
                            possible_kinds: KindSet::from_kind(ValueKind::Array),
                            heap_shape: Some(Self::array_prototype_shape()),
                            function_targets: FunctionTargetKnowledge::none(),
                        }),
                    );
                    object.properties.insert(
                        "from".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ArrayFrom.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "fromAsync".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ArrayFromAsync.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "of".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ArrayOf.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "isArray".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ArrayIsArray.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        shape_namespace_key(WellKnownSymbol::Species),
                        ObjectShapeProperty::Accessor {
                            getter: Some(ObjectAccessorShape {
                                function_id: StandardBuiltinId::ArraySpeciesGetter.function_id(),
                            }),
                            setter: None,
                        },
                    );
                }
                StandardBuiltinId::ArrayBufferConstructor
                | StandardBuiltinId::SharedArrayBufferConstructor => {
                    let prototype_shape =
                        if matches!(builtin, StandardBuiltinId::ArrayBufferConstructor) {
                            Self::array_buffer_prototype_shape()
                        } else {
                            Self::shared_array_buffer_prototype_shape()
                        };
                    if matches!(builtin, StandardBuiltinId::ArrayBufferConstructor) {
                        object.properties.insert(
                            "isView".to_string(),
                            ObjectShapeProperty::Data(
                                Self::function_value_info_with_constructable(
                                    StandardBuiltinId::ArrayBufferIsView.function_id(),
                                    false,
                                ),
                            ),
                        );
                        object.properties.insert(
                            shape_namespace_key(WellKnownSymbol::Species),
                            ObjectShapeProperty::Accessor {
                                getter: Some(ObjectAccessorShape {
                                    function_id: StandardBuiltinId::ArrayBufferSpeciesGetter
                                        .function_id(),
                                }),
                                setter: None,
                            },
                        );
                    }
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            prototype_shape,
                        ))),
                    );
                }
                StandardBuiltinId::DataViewConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::data_view_prototype_shape(),
                        ))),
                    );
                }
                StandardBuiltinId::DateConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::date_prototype_shape(),
                        ))),
                    );
                    object.properties.insert(
                        "now".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::DateNow.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "parse".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::DateParse.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "UTC".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::DateUtc.function_id(),
                            false,
                        )),
                    );
                }
                StandardBuiltinId::TemporalInstantConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::temporal_instant_prototype_shape(),
                        ))),
                    );
                    object.properties.insert(
                        "from".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalInstantFrom.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "compare".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalInstantCompare.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "fromEpochMilliseconds".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalInstantFromEpochMilliseconds.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "fromEpochNanoseconds".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalInstantFromEpochNanoseconds.function_id(),
                            false,
                        )),
                    );
                }
                StandardBuiltinId::TemporalPlainDateConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::temporal_plain_date_prototype_shape(),
                        ))),
                    );
                    object.properties.insert(
                        "from".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalPlainDateFrom.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "compare".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalPlainDateCompare.function_id(),
                            false,
                        )),
                    );
                }
                StandardBuiltinId::TemporalPlainYearMonthConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::temporal_plain_year_month_prototype_shape(),
                        ))),
                    );
                    object.properties.insert(
                        "from".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalPlainYearMonthFrom.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "compare".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalPlainYearMonthCompare.function_id(),
                            false,
                        )),
                    );
                }
                StandardBuiltinId::TemporalPlainMonthDayConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::temporal_plain_month_day_prototype_shape(),
                        ))),
                    );
                    object.properties.insert(
                        "from".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalPlainMonthDayFrom.function_id(),
                            false,
                        )),
                    );
                }
                StandardBuiltinId::TemporalPlainDateTimeConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::temporal_plain_date_time_prototype_shape(),
                        ))),
                    );
                    object.properties.insert(
                        "from".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalPlainDateTimeFrom.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "compare".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalPlainDateTimeCompare.function_id(),
                            false,
                        )),
                    );
                }
                StandardBuiltinId::TemporalPlainTimeConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::temporal_plain_time_prototype_shape(),
                        ))),
                    );
                    object.properties.insert(
                        "from".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalPlainTimeFrom.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "compare".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalPlainTimeCompare.function_id(),
                            false,
                        )),
                    );
                }
                StandardBuiltinId::TemporalDurationConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::temporal_duration_prototype_shape(),
                        ))),
                    );
                    object.properties.insert(
                        "from".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalDurationFrom.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "compare".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalDurationCompare.function_id(),
                            false,
                        )),
                    );
                }
                StandardBuiltinId::TemporalZonedDateTimeConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::temporal_zoned_date_time_prototype_shape(),
                        ))),
                    );
                    object.properties.insert(
                        "from".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalZonedDateTimeFrom.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "compare".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::TemporalZonedDateTimeCompare.function_id(),
                            false,
                        )),
                    );
                }
                StandardBuiltinId::RegExpConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::regexp_prototype_shape(),
                        ))),
                    );
                    object.properties.insert(
                        shape_namespace_key(WellKnownSymbol::Species),
                        ObjectShapeProperty::Accessor {
                            getter: Some(ObjectAccessorShape {
                                function_id: StandardBuiltinId::RegExpSpeciesGetter.function_id(),
                            }),
                            setter: None,
                        },
                    );
                    object.properties.insert(
                        "escape".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::RegExpEscape.function_id(),
                            false,
                        )),
                    );
                    for name in ["input", "$_"] {
                        object.properties.insert(
                            name.to_string(),
                            ObjectShapeProperty::Accessor {
                                getter: Some(ObjectAccessorShape {
                                    function_id: StandardBuiltinId::RegExpLegacyStaticGetter
                                        .function_id(),
                                }),
                                setter: Some(ObjectAccessorShape {
                                    function_id: StandardBuiltinId::RegExpLegacyStaticSetter
                                        .function_id(),
                                }),
                            },
                        );
                    }
                    for name in [
                        "lastMatch",
                        "$&",
                        "lastParen",
                        "$+",
                        "leftContext",
                        "$`",
                        "rightContext",
                        "$'",
                    ] {
                        object.properties.insert(
                            name.to_string(),
                            ObjectShapeProperty::Accessor {
                                getter: Some(ObjectAccessorShape {
                                    function_id: StandardBuiltinId::RegExpLegacyStaticGetter
                                        .function_id(),
                                }),
                                setter: None,
                            },
                        );
                    }
                    for index in 1..=9 {
                        object.properties.insert(
                            format!("${index}"),
                            ObjectShapeProperty::Accessor {
                                getter: Some(ObjectAccessorShape {
                                    function_id: StandardBuiltinId::RegExpLegacyStaticGetter
                                        .function_id(),
                                }),
                                setter: None,
                            },
                        );
                    }
                }
                StandardBuiltinId::Float64ArrayConstructor
                | StandardBuiltinId::Float32ArrayConstructor
                | StandardBuiltinId::Float16ArrayConstructor
                | StandardBuiltinId::Int32ArrayConstructor
                | StandardBuiltinId::Int16ArrayConstructor
                | StandardBuiltinId::Int8ArrayConstructor
                | StandardBuiltinId::Uint32ArrayConstructor
                | StandardBuiltinId::Uint16ArrayConstructor
                | StandardBuiltinId::Uint8ArrayConstructor
                | StandardBuiltinId::Uint8ClampedArrayConstructor
                | StandardBuiltinId::BigInt64ArrayConstructor
                | StandardBuiltinId::BigUint64ArrayConstructor => {
                    object.prototype = Some(Self::standard_builtin_function_shape(
                        StandardBuiltinId::TypedArrayConstructor,
                    ));
                    if builtin == StandardBuiltinId::Uint8ArrayConstructor {
                        for (name, method) in UINT8_ARRAY_CODEC_STATIC_MEMBERS {
                            object.properties.insert(
                                name.to_string(),
                                ObjectShapeProperty::Data(
                                    Self::function_value_info_with_constructable(
                                        method.function_id(),
                                        false,
                                    ),
                                ),
                            );
                        }
                    }
                    object.properties.insert(
                        "BYTES_PER_ELEMENT".to_string(),
                        ObjectShapeProperty::Data(ValueInfo::new(ValueKind::Number)),
                    );
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::typed_array_constructor_prototype_shape(builtin),
                        ))),
                    );
                }
                StandardBuiltinId::EvalErrorConstructor
                | StandardBuiltinId::AggregateErrorConstructor
                | StandardBuiltinId::SuppressedErrorConstructor
                | StandardBuiltinId::RangeErrorConstructor
                | StandardBuiltinId::SyntaxErrorConstructor
                | StandardBuiltinId::TypeErrorConstructor
                | StandardBuiltinId::URIErrorConstructor
                | StandardBuiltinId::ReferenceErrorConstructor => {
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::standard_error_prototype_shape(builtin),
                        ))),
                    );
                }
                StandardBuiltinId::ErrorConstructor => {
                    object.properties.insert(
                        "isError".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ErrorIsError.function_id(),
                            false,
                        )),
                    );
                    object.properties.insert(
                        "prototype".to_string(),
                        ObjectShapeProperty::Data(Self::value_info_from_shape(Some(
                            Self::standard_error_prototype_shape(builtin),
                        ))),
                    );
                }
                StandardBuiltinId::ProxyConstructor => {
                    // Proxy.revocable is only reachable through this static shape entry
                    // (there is no `Proxy.prototype` object to hang it off of, unlike the
                    // Error family above). Without a `function_targets` entry here,
                    // `Proxy.revocable(...)` compiles as a fully dynamic property
                    // read + indirect call and never surfaces a reference to the
                    // `ProxyRevocable` builtin anywhere in the IR, so the reachability
                    // scan (`script_references_standard_builtin`) treats it as unused and
                    // stubs it out — leaving the real `Proxy.revocable` property missing
                    // at runtime even though the script calls it directly.
                    object.properties.insert(
                        "revocable".to_string(),
                        ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                            StandardBuiltinId::ProxyRevocable.function_id(),
                            false,
                        )),
                    );
                }
                StandardBuiltinId::FunctionPrototypeCall
                | StandardBuiltinId::FunctionPrototypeApply
                | StandardBuiltinId::FunctionPrototypeBind
                | StandardBuiltinId::FunctionPrototypeSymbolHasInstance
                | StandardBuiltinId::FunctionPrototypeToString
                | StandardBuiltinId::DataViewPrototypeBufferGetter
                | StandardBuiltinId::DataViewPrototypeByteLengthGetter
                | StandardBuiltinId::DataViewPrototypeByteOffsetGetter
                | StandardBuiltinId::DataViewPrototypeGetUint8
                | StandardBuiltinId::DataViewPrototypeSetUint8
                | StandardBuiltinId::DataViewPrototypeGetInt8
                | StandardBuiltinId::DataViewPrototypeSetInt8
                | StandardBuiltinId::ErrorPrototypeToString => {}
                _ => {}
            }
        }
        shape
    }

    pub(super) fn standard_builtin_value_info(builtin: StandardBuiltinId) -> ValueInfo {
        ValueInfo {
            kind: ValueKind::Function,
            possible_kinds: KindSet::from_kind(ValueKind::Function),
            heap_shape: Some(Self::standard_builtin_function_shape(builtin)),
            function_targets: FunctionTargetKnowledge::exact(builtin.function_id()),
        }
    }

    pub(crate) fn iterator_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        properties.insert(
            "constructor".to_string(),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::IteratorPrototypeConstructorGetter
                        .function_id(),
                }),
                setter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::IteratorPrototypeConstructorSetter
                        .function_id(),
                }),
            },
        );
        properties.insert(
            shape_namespace_key(WellKnownSymbol::Iterator),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::ArrayIteratorIdentity,
            )),
        );
        properties.insert(
            shape_namespace_key(WellKnownSymbol::Dispose),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorPrototypeSymbolDispose,
            )),
        );
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Accessor {
                getter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::IteratorPrototypeToStringTagGetter
                        .function_id(),
                }),
                setter: Some(ObjectAccessorShape {
                    function_id: StandardBuiltinId::IteratorPrototypeToStringTagSetter
                        .function_id(),
                }),
            },
        );
        properties.insert(
            "toArray".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorPrototypeToArray,
            )),
        );
        properties.insert(
            "forEach".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorPrototypeForEach,
            )),
        );
        for (name, builtin) in [
            ("includes", StandardBuiltinId::IteratorPrototypeIncludes),
            ("join", StandardBuiltinId::IteratorPrototypeJoin),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::standard_builtin_value_info(builtin)),
            );
        }
        properties.insert(
            "every".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorPrototypeEvery,
            )),
        );
        properties.insert(
            "some".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorPrototypeSome,
            )),
        );
        properties.insert(
            "find".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorPrototypeFind,
            )),
        );
        properties.insert(
            "reduce".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorPrototypeReduce,
            )),
        );
        properties.insert(
            "map".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorPrototypeMap,
            )),
        );
        properties.insert(
            "filter".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorPrototypeFilter,
            )),
        );
        properties.insert(
            "flatMap".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorPrototypeFlatMap,
            )),
        );
        properties.insert(
            "take".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorPrototypeTake,
            )),
        );
        properties.insert(
            "drop".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorPrototypeDrop,
            )),
        );
        properties.insert(
            "chunks".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorPrototypeChunks,
            )),
        );
        properties.insert(
            "windows".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorPrototypeWindows,
            )),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::IntrinsicPrototype(
                StandardBuiltinId::IteratorConstructor,
            ),
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn iterator_helper_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        properties.insert(
            "next".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorHelperNext,
            )),
        );
        properties.insert(
            "return".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorHelperReturn,
            )),
        );
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(ValueInfo::new(ValueKind::String)),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::UntrackedIntrinsicPrototype,
            prototype: Some(Self::iterator_prototype_shape()),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn iterator_from_wrapper_prototype_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        properties.insert(
            "return".to_string(),
            ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                StandardBuiltinId::IteratorFromWrapperReturn,
            )),
        );
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::UntrackedIntrinsicPrototype,
            prototype: Some(Self::iterator_prototype_shape()),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn iterator_take_helper_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::iterator_helper_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn iterator_zip_helper_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::iterator_helper_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn iterator_concat_helper_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::iterator_helper_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn iterator_drop_helper_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::iterator_helper_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn iterator_helper_info() -> ValueInfo {
        ValueInfo {
            kind: ValueKind::Object,
            possible_kinds: KindSet::from_kind(ValueKind::Object),
            heap_shape: Some(Self::iterator_helper_instance_shape()),
            function_targets: FunctionTargetKnowledge::none(),
        }
    }

    pub(super) fn iterator_helper_instance_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::iterator_helper_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn iterator_map_helper_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::iterator_helper_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn iterator_filter_helper_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::iterator_helper_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn iterator_flat_map_helper_shape() -> Box<HeapShape> {
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Self::iterator_helper_prototype_shape()),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn regexp_string_iterator_instance_shape() -> Box<HeapShape> {
        let prototype = HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::UntrackedIntrinsicPrototype,
            prototype: Some(Self::iterator_prototype_shape()),
            properties: BTreeMap::from([(
                "next".to_string(),
                ObjectShapeProperty::Data(Self::standard_builtin_value_info(
                    StandardBuiltinId::RegExpStringIteratorNext,
                )),
            )]),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        });
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Box::new(prototype)),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn generator_instance_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, builtin) in [
            ("next", StandardBuiltinId::GeneratorPrototypeNext),
            ("return", StandardBuiltinId::GeneratorPrototypeReturn),
            ("throw", StandardBuiltinId::GeneratorPrototypeThrow),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::standard_builtin_value_info(builtin)),
            );
        }
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Box::new(HeapShape::Object(ObjectShape {
                provenance: HeapShapeProvenance::UntrackedIntrinsicPrototype,
                prototype: Some(Self::iterator_prototype_shape()),
                properties,
                private_brands: BTreeSet::new(),
                boxed_primitive: None,
            }))),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn async_generator_instance_shape() -> Box<HeapShape> {
        let mut properties = BTreeMap::new();
        for (name, builtin) in [
            ("next", StandardBuiltinId::AsyncGeneratorPrototypeNext),
            ("return", StandardBuiltinId::AsyncGeneratorPrototypeReturn),
            ("throw", StandardBuiltinId::AsyncGeneratorPrototypeThrow),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::standard_builtin_value_info(builtin)),
            );
        }
        Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Box::new(HeapShape::Object(ObjectShape {
                provenance: HeapShapeProvenance::UntrackedIntrinsicPrototype,
                prototype: None,
                properties,
                private_brands: BTreeSet::new(),
                boxed_primitive: None,
            }))),
            properties: BTreeMap::new(),
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))
    }

    pub(super) fn reflect_object_value_info() -> ValueInfo {
        let mut properties = BTreeMap::new();
        properties.insert(
            "construct".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ReflectConstruct.function_id(),
                false,
            )),
        );
        properties.insert(
            "apply".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ReflectApply.function_id(),
                false,
            )),
        );
        properties.insert(
            "get".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ReflectGet.function_id(),
                false,
            )),
        );
        properties.insert(
            "getPrototypeOf".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ReflectGetPrototypeOf.function_id(),
                false,
            )),
        );
        properties.insert(
            "getOwnPropertyDescriptor".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ReflectGetOwnPropertyDescriptor.function_id(),
                false,
            )),
        );
        properties.insert(
            "set".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ReflectSet.function_id(),
                false,
            )),
        );
        properties.insert(
            "has".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ReflectHas.function_id(),
                false,
            )),
        );
        properties.insert(
            "defineProperty".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ReflectDefineProperty.function_id(),
                false,
            )),
        );
        properties.insert(
            "deleteProperty".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ReflectDeleteProperty.function_id(),
                false,
            )),
        );
        properties.insert(
            "isExtensible".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ReflectIsExtensible.function_id(),
                false,
            )),
        );
        properties.insert(
            "preventExtensions".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ReflectPreventExtensions.function_id(),
                false,
            )),
        );
        properties.insert(
            "setPrototypeOf".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ReflectSetPrototypeOf.function_id(),
                false,
            )),
        );
        properties.insert(
            "ownKeys".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::ReflectOwnKeys.function_id(),
                false,
            )),
        );
        Self::value_info_from_shape(Some(Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))))
    }

    pub(super) fn math_object_value_info() -> ValueInfo {
        let mut properties = BTreeMap::new();
        for name in [
            "E", "LN10", "LN2", "LOG10E", "LOG2E", "PI", "SQRT1_2", "SQRT2",
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(ValueInfo::new(ValueKind::Number)),
            );
        }
        for (name, builtin) in [
            ("abs", StandardBuiltinId::MathAbs),
            ("acos", StandardBuiltinId::MathAcos),
            ("acosh", StandardBuiltinId::MathAcosh),
            ("asin", StandardBuiltinId::MathAsin),
            ("asinh", StandardBuiltinId::MathAsinh),
            ("atan", StandardBuiltinId::MathAtan),
            ("atan2", StandardBuiltinId::MathAtan2),
            ("atanh", StandardBuiltinId::MathAtanh),
            ("cbrt", StandardBuiltinId::MathCbrt),
            ("ceil", StandardBuiltinId::MathCeil),
            ("clz32", StandardBuiltinId::MathClz32),
            ("cos", StandardBuiltinId::MathCos),
            ("cosh", StandardBuiltinId::MathCosh),
            ("exp", StandardBuiltinId::MathExp),
            ("expm1", StandardBuiltinId::MathExpm1),
            ("f16round", StandardBuiltinId::MathF16Round),
            ("floor", StandardBuiltinId::MathFloor),
            ("fround", StandardBuiltinId::MathFround),
            ("hypot", StandardBuiltinId::MathHypot),
            ("imul", StandardBuiltinId::MathImul),
            ("log", StandardBuiltinId::MathLog),
            ("log10", StandardBuiltinId::MathLog10),
            ("log1p", StandardBuiltinId::MathLog1p),
            ("log2", StandardBuiltinId::MathLog2),
            ("pow", StandardBuiltinId::MathPow),
            ("random", StandardBuiltinId::MathRandom),
            ("round", StandardBuiltinId::MathRound),
            ("sign", StandardBuiltinId::MathSign),
            ("sin", StandardBuiltinId::MathSin),
            ("sinh", StandardBuiltinId::MathSinh),
            ("sqrt", StandardBuiltinId::MathSqrt),
            ("sumPrecise", StandardBuiltinId::MathSumPrecise),
            ("tan", StandardBuiltinId::MathTan),
            ("tanh", StandardBuiltinId::MathTanh),
            ("trunc", StandardBuiltinId::MathTrunc),
            ("min", StandardBuiltinId::MathMin),
            ("max", StandardBuiltinId::MathMax),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::standard_builtin_value_info(builtin)),
            );
        }
        Self::value_info_from_shape(Some(Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))))
    }

    pub(super) fn json_object_value_info() -> ValueInfo {
        let mut properties = BTreeMap::new();
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info(JSON_NAME)),
        );
        for (name, builtin) in [
            ("parse", StandardBuiltinId::JsonParse),
            ("stringify", StandardBuiltinId::JsonStringify),
            ("rawJSON", StandardBuiltinId::JsonRawJson),
            ("isRawJSON", StandardBuiltinId::JsonIsRawJson),
        ] {
            properties.insert(
                name.to_string(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    builtin.function_id(),
                    false,
                )),
            );
        }
        Self::value_info_from_shape(Some(Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))))
    }

    /// The `Temporal.Now` namespace object: a plain object holding only the
    /// clock-reading functions this backend actually implements.
    pub(super) fn temporal_now_object_value_info() -> ValueInfo {
        let mut properties = BTreeMap::new();
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info("Temporal.Now")),
        );
        for (name, builtin) in TEMPORAL_NOW_NAMESPACE_MEMBERS {
            properties.insert(
                (*name).to_string(),
                ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                    builtin.function_id(),
                    false,
                )),
            );
        }
        Self::value_info_from_shape(Some(Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))))
    }

    pub(super) fn temporal_object_value_info() -> ValueInfo {
        let mut properties = BTreeMap::from([
            (
                TEMPORAL_NOW_NAME.to_string(),
                ObjectShapeProperty::Data(Self::temporal_now_object_value_info()),
            ),
            (
                shape_namespace_key(WellKnownSymbol::ToStringTag),
                ObjectShapeProperty::Data(Self::string_value_info(TEMPORAL_NAME)),
            ),
        ]);
        for (name, builtin) in TEMPORAL_NAMESPACE_CONSTRUCTORS {
            properties.insert(
                (*name).to_string(),
                ObjectShapeProperty::Data(Self::standard_builtin_value_info(*builtin)),
            );
        }
        Self::value_info_from_shape(Some(Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))))
    }

    /// The `Intl` namespace shape.
    ///
    /// The constructor-valued members come from `INTL_NAMESPACE_CONSTRUCTORS`,
    /// which `FunctionBuilder::init_intl_object` also walks, so the shape and
    /// the installer cannot disagree about what `Intl` has. They used to be two
    /// hand-maintained lists and they drifted — see the slice's own comment.
    pub(super) fn intl_object_value_info() -> ValueInfo {
        let mut properties = BTreeMap::from([(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info(INTL_NAME)),
        )]);
        for (name, builtin) in INTL_NAMESPACE_METHODS {
            properties.insert(
                (*name).to_string(),
                ObjectShapeProperty::Data(Self::standard_builtin_value_info(*builtin)),
            );
        }
        for (name, builtin) in INTL_NAMESPACE_CONSTRUCTORS {
            properties.insert(
                (*name).to_string(),
                ObjectShapeProperty::Data(Self::standard_builtin_value_info(*builtin)),
            );
        }
        Self::value_info_from_shape(Some(Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))))
    }

    pub(super) fn atomics_object_value_info() -> ValueInfo {
        let mut properties = BTreeMap::new();
        properties.insert(
            shape_namespace_key(WellKnownSymbol::ToStringTag),
            ObjectShapeProperty::Data(Self::string_value_info(ATOMICS_NAME)),
        );
        properties.insert(
            "add".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::AtomicsAdd.function_id(),
                false,
            )),
        );
        properties.insert(
            "and".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::AtomicsAnd.function_id(),
                false,
            )),
        );
        properties.insert(
            "compareExchange".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::AtomicsCompareExchange.function_id(),
                false,
            )),
        );
        properties.insert(
            "exchange".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::AtomicsExchange.function_id(),
                false,
            )),
        );
        properties.insert(
            "isLockFree".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::AtomicsIsLockFree.function_id(),
                false,
            )),
        );
        properties.insert(
            "load".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::AtomicsLoad.function_id(),
                false,
            )),
        );
        properties.insert(
            "notify".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::AtomicsNotify.function_id(),
                false,
            )),
        );
        properties.insert(
            "or".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::AtomicsOr.function_id(),
                false,
            )),
        );
        properties.insert(
            "pause".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::AtomicsPause.function_id(),
                false,
            )),
        );
        properties.insert(
            "store".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::AtomicsStore.function_id(),
                false,
            )),
        );
        properties.insert(
            "sub".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::AtomicsSub.function_id(),
                false,
            )),
        );
        properties.insert(
            "wait".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::AtomicsWait.function_id(),
                false,
            )),
        );
        properties.insert(
            "waitAsync".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::AtomicsWaitAsync.function_id(),
                false,
            )),
        );
        properties.insert(
            "xor".to_string(),
            ObjectShapeProperty::Data(Self::function_value_info_with_constructable(
                StandardBuiltinId::AtomicsXor.function_id(),
                false,
            )),
        );
        Self::value_info_from_shape(Some(Box::new(HeapShape::Object(ObjectShape {
            provenance: HeapShapeProvenance::Program,
            prototype: Some(Box::new(Self::empty_object_shape())),
            properties,
            private_brands: BTreeSet::new(),
            boxed_primitive: None,
        }))))
    }

    pub(super) fn empty_dynamic_function_info(kind: DynamicFunctionKind) -> ValueInfo {
        ValueInfo {
            kind: ValueKind::Function,
            possible_kinds: KindSet::from_kind(ValueKind::Function),
            heap_shape: match kind {
                DynamicFunctionKind::Ordinary => Some(Self::function_heap_shape(true)),
                DynamicFunctionKind::Generator
                | DynamicFunctionKind::Async
                | DynamicFunctionKind::AsyncGenerator => None,
            },
            function_targets: FunctionTargetKnowledge::unknown(),
        }
    }

    pub(super) fn standard_builtin_signature(builtin: StandardBuiltinId) -> FunctionSignature {
        let (return_kind, return_possible_kinds, return_shape, constructor_instance) = match builtin
        {
            StandardBuiltinId::ReadableStreamConstructor
            | StandardBuiltinId::ReadableStreamDefaultReaderConstructor
            | StandardBuiltinId::ReadableByteStreamControllerConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                ValueInfo::new(ValueKind::Object),
            ),
            StandardBuiltinId::ReadableStreamGetReader => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ReadableStreamCancel
            | StandardBuiltinId::ReadableStreamDefaultReaderRead
            | StandardBuiltinId::ReadableStreamDefaultReaderCancel
            | StandardBuiltinId::ReadableStreamDefaultReaderClosed => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::promise_instance_shape()),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ReadableStreamDefaultReaderReleaseLock
            | StandardBuiltinId::ReadableByteStreamControllerEnqueue
            | StandardBuiltinId::ReadableByteStreamControllerClose
            | StandardBuiltinId::ReadableByteStreamControllerError
            | StandardBuiltinId::ReadableStreamStartFulfilled
            | StandardBuiltinId::ReadableStreamStartRejected
            | StandardBuiltinId::ReadableStreamPullFulfilled
            | StandardBuiltinId::ReadableStreamPullRejected
            | StandardBuiltinId::ReadableStreamCancelFulfilled
            | StandardBuiltinId::ReadableStreamPullIfNeeded => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ReadableStreamLocked => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ReadableByteStreamControllerDesiredSize => (
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::Number).union(KindSet::from_kind(ValueKind::Null)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::AbortControllerConstructor
            | StandardBuiltinId::AbortSignalConstructor
            | StandardBuiltinId::EventTargetConstructor
            | StandardBuiltinId::EventConstructor
            | StandardBuiltinId::DOMExceptionConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                ValueInfo::new(ValueKind::Object),
            ),
            StandardBuiltinId::AbortControllerPrototypeAbort
            | StandardBuiltinId::AbortControllerPrototypeSignalGetter
            | StandardBuiltinId::AbortSignalPrototypeThrowIfAborted
            | StandardBuiltinId::AbortSignalAbort
            | StandardBuiltinId::AbortSignalPrototypeAbortedGetter
            | StandardBuiltinId::AbortSignalPrototypeReasonGetter
            | StandardBuiltinId::AbortSignalPrototypeOnabortGetter
            | StandardBuiltinId::AbortSignalPrototypeOnabortSetter
            | StandardBuiltinId::EventTargetPrototypeAddEventListener
            | StandardBuiltinId::EventTargetPrototypeRemoveEventListener
            | StandardBuiltinId::EventTargetPrototypeDispatchEvent
            | StandardBuiltinId::EventPrototypePreventDefault
            | StandardBuiltinId::EventPrototypeStopPropagation
            | StandardBuiltinId::EventPrototypeStopImmediatePropagation
            | StandardBuiltinId::EventPrototypeTypeGetter
            | StandardBuiltinId::EventPrototypeTargetGetter
            | StandardBuiltinId::EventPrototypeCurrentTargetGetter
            | StandardBuiltinId::EventPrototypeDefaultPreventedGetter
            | StandardBuiltinId::EventPrototypeEventPhaseGetter
            | StandardBuiltinId::EventPrototypeBubblesGetter
            | StandardBuiltinId::EventPrototypeCancelableGetter
            | StandardBuiltinId::EventPrototypeComposedGetter
            | StandardBuiltinId::EventPrototypeIsTrustedGetter
            | StandardBuiltinId::DOMExceptionPrototypeNameGetter
            | StandardBuiltinId::DOMExceptionPrototypeMessageGetter
            | StandardBuiltinId::DOMExceptionPrototypeCodeGetter => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ShadowRealmConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Self::shadow_realm_instance_info().heap_shape,
                Self::shadow_realm_instance_info(),
            ),
            StandardBuiltinId::ShadowRealmPrototypeImportValue => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::promise_instance_shape()),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ShadowRealmPrototypeEvaluate
            | StandardBuiltinId::ShadowRealmWrappedFunctionCall
            | StandardBuiltinId::ShadowRealmImportFulfilled
            | StandardBuiltinId::ShadowRealmImportRejected => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::QueueMicrotask | StandardBuiltinId::ClearTimeout => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SetTimeout => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::FunctionConstructor => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::function_heap_shape(true)),
                Self::empty_dynamic_function_info(DynamicFunctionKind::Ordinary),
            ),
            StandardBuiltinId::FunctionPrototype => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::FunctionPrototypeSymbolHasInstance => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TypedArrayConstructor => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::FunctionPrototypeCall
            | StandardBuiltinId::FunctionPrototypeApply => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::FunctionPrototypeBind => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::FunctionPrototypeToString => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::EvalFunction => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ObjectConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Box::new(Self::empty_object_shape())),
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::ObjectGroupBy | StandardBuiltinId::ObjectFromEntries => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ObjectAssign => (
                ValueKind::Object,
                Self::object_like_kind_set(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ObjectCreate => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Box::new(Self::empty_object_shape())),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ObjectGetPrototypeOf => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object).union(KindSet::from_kind(ValueKind::Null)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ObjectSetPrototypeOf => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ObjectDefineProperty | StandardBuiltinId::ObjectDefineProperties => {
                (
                    ValueKind::Object,
                    KindSet::from_kind(ValueKind::Object),
                    Some(Box::new(Self::empty_object_shape())),
                    ValueInfo::undefined(),
                )
            }
            StandardBuiltinId::ObjectGetOwnPropertyDescriptor
            | StandardBuiltinId::ReflectGetOwnPropertyDescriptor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                Some(Self::generic_property_descriptor_shape()),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ObjectGetOwnPropertyDescriptors => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Box::new(Self::empty_object_shape())),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ObjectPrototypeLookupGetter
            | StandardBuiltinId::ObjectPrototypeLookupSetter => (
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::Function)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ObjectPrototypeProtoGetter => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object).union(KindSet::from_kind(ValueKind::Null)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ObjectPrototypeDefineGetter
            | StandardBuiltinId::ObjectPrototypeDefineSetter
            | StandardBuiltinId::ObjectPrototypeProtoSetter => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ObjectGetOwnPropertyNames
            | StandardBuiltinId::ObjectGetOwnPropertySymbols
            | StandardBuiltinId::ObjectKeys
            | StandardBuiltinId::ObjectValues
            | StandardBuiltinId::ObjectEntries => (
                ValueKind::Array,
                KindSet::from_kind(ValueKind::Array),
                Some(Box::new(HeapShape::Array(ArrayShape::default()))),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ObjectIs
            | StandardBuiltinId::ObjectHasOwn
            | StandardBuiltinId::ObjectIsSealed
            | StandardBuiltinId::ObjectIsFrozen
            | StandardBuiltinId::ObjectIsExtensible
            | StandardBuiltinId::ObjectPrototypeHasOwnProperty
            | StandardBuiltinId::ObjectPrototypePropertyIsEnumerable
            | StandardBuiltinId::ObjectPrototypeIsPrototypeOf
            | StandardBuiltinId::JsonIsRawJson
            | StandardBuiltinId::AtomicsIsLockFree
            | StandardBuiltinId::StringPrototypeEndsWith
            | StandardBuiltinId::StringPrototypeIncludes
            | StandardBuiltinId::StringPrototypeStartsWith
            | StandardBuiltinId::ErrorIsError => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::AtomicsAdd
            | StandardBuiltinId::AtomicsAnd
            | StandardBuiltinId::AtomicsCompareExchange
            | StandardBuiltinId::AtomicsExchange
            | StandardBuiltinId::AtomicsLoad
            | StandardBuiltinId::AtomicsOr
            | StandardBuiltinId::AtomicsSub
            | StandardBuiltinId::AtomicsStore
            | StandardBuiltinId::AtomicsXor => (
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::Number).union(KindSet::from_kind(ValueKind::BigInt)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::AtomicsNotify
            | StandardBuiltinId::StringPrototypeIndexOf
            | StandardBuiltinId::StringPrototypeLastIndexOf
            | StandardBuiltinId::StringPrototypeCharCodeAt
            | StandardBuiltinId::StringPrototypeLocaleCompare
            | StandardBuiltinId::IntlCollatorBoundCompare => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::AtomicsWait => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::AtomicsPause => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::RegExpPrototypeSymbolMatchAll
            | StandardBuiltinId::AtomicsWaitAsync => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                if builtin == StandardBuiltinId::AtomicsWaitAsync {
                    Some(Box::new(HeapShape::Object(ObjectShape {
                        provenance: HeapShapeProvenance::Program,
                        prototype: Some(Box::new(Self::empty_object_shape())),
                        properties: BTreeMap::from([
                            (
                                "async".to_string(),
                                ObjectShapeProperty::Data(ValueInfo::new(ValueKind::Boolean)),
                            ),
                            (
                                "value".to_string(),
                                ObjectShapeProperty::Data(ValueInfo {
                                    kind: ValueKind::Dynamic,
                                    possible_kinds: KindSet::from_kind(ValueKind::String)
                                        .union(KindSet::from_kind(ValueKind::Object)),
                                    heap_shape: None,
                                    function_targets: FunctionTargetKnowledge::none(),
                                }),
                            ),
                        ]),
                        private_brands: BTreeSet::new(),
                        boxed_primitive: None,
                    })))
                } else if builtin == StandardBuiltinId::RegExpPrototypeSymbolMatchAll {
                    Some(Self::regexp_string_iterator_instance_shape())
                } else {
                    None
                },
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IntlDisplayNamesPrototypeOf | StandardBuiltinId::JsonStringify => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IntlSegmentsPrototypeContaining => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::JsonParse => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::JsonRawJson => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Box::new(Self::raw_json_object_shape())),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ObjectPrototypeToString => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ObjectPrototypeToLocaleString => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ObjectPrototypeValueOf => (
                ValueKind::Object,
                Self::object_like_kind_set(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ObjectSeal
            | StandardBuiltinId::ObjectFreeze
            | StandardBuiltinId::ObjectPreventExtensions => (
                ValueKind::Object,
                Self::object_like_kind_set(),
                Some(Box::new(Self::empty_object_shape())),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ProxyConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ProxyRevocable => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ProxyRevoke => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ReflectConstruct => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ReflectApply => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ReflectGet => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ReflectGetPrototypeOf => (
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::Object).union(KindSet::from_kind(ValueKind::Null)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ReflectSet => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ReflectHas => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ReflectDefineProperty => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ReflectDeleteProperty => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ReflectIsExtensible => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ReflectPreventExtensions => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ReflectSetPrototypeOf => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ReflectOwnKeys => (
                ValueKind::Array,
                KindSet::from_kind(ValueKind::Array),
                Some(Box::new(HeapShape::Array(ArrayShape::default()))),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayConstructor => (
                ValueKind::Array,
                KindSet::from_kind(ValueKind::Array),
                Some(Box::new(HeapShape::Array(ArrayShape::default()))),
                Self::fresh_constructed_array_instance_info(),
            ),
            StandardBuiltinId::ArrayFrom => (
                ValueKind::Dynamic,
                Self::object_like_kind_set(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayFromAsync => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::promise_instance_shape()),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayOf => (
                ValueKind::Dynamic,
                Self::object_like_kind_set(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorFrom => (
                ValueKind::Object,
                Self::object_like_kind_set(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorConcat => (
                ValueKind::Object,
                Self::object_like_kind_set(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorZip => (
                ValueKind::Object,
                Self::object_like_kind_set(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorZipKeyed => (
                ValueKind::Object,
                Self::object_like_kind_set(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayIsArray => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::NumberIsInteger
            | StandardBuiltinId::NumberIsSafeInteger
            | StandardBuiltinId::NumberIsFinite
            | StandardBuiltinId::NumberIsNaN
            | StandardBuiltinId::GlobalIsFinite
            | StandardBuiltinId::GlobalIsNaN => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::NumberPrototypeToExponential
            | StandardBuiltinId::NumberPrototypeToFixed
            | StandardBuiltinId::NumberPrototypeToPrecision
            | StandardBuiltinId::NumberPrototypeToString
            | StandardBuiltinId::NumberPrototypeToLocaleString => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::NumberPrototypeValueOf => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::MathAbs
            | StandardBuiltinId::MathAcos
            | StandardBuiltinId::MathAcosh
            | StandardBuiltinId::MathAsin
            | StandardBuiltinId::MathAsinh
            | StandardBuiltinId::MathAtan
            | StandardBuiltinId::MathAtan2
            | StandardBuiltinId::MathAtanh
            | StandardBuiltinId::MathCbrt
            | StandardBuiltinId::MathCeil
            | StandardBuiltinId::MathClz32
            | StandardBuiltinId::MathCos
            | StandardBuiltinId::MathCosh
            | StandardBuiltinId::MathExp
            | StandardBuiltinId::MathExpm1
            | StandardBuiltinId::MathF16Round
            | StandardBuiltinId::MathFloor
            | StandardBuiltinId::MathFround
            | StandardBuiltinId::MathHypot
            | StandardBuiltinId::MathImul
            | StandardBuiltinId::MathLog
            | StandardBuiltinId::MathLog10
            | StandardBuiltinId::MathLog1p
            | StandardBuiltinId::MathLog2
            | StandardBuiltinId::MathPow
            | StandardBuiltinId::MathRandom
            | StandardBuiltinId::MathRound
            | StandardBuiltinId::MathSign
            | StandardBuiltinId::MathSin
            | StandardBuiltinId::MathSinh
            | StandardBuiltinId::MathSqrt
            | StandardBuiltinId::MathSumPrecise
            | StandardBuiltinId::MathTan
            | StandardBuiltinId::MathTanh
            | StandardBuiltinId::MathTrunc
            | StandardBuiltinId::MathMin
            | StandardBuiltinId::MathMax => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeConcat | StandardBuiltinId::ArrayPrototypeSplice => (
                ValueKind::Dynamic,
                Self::object_like_kind_set(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeSlice => (
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::Array)
                    .union(KindSet::from_kind(ValueKind::Object))
                    .union(KindSet::from_kind(ValueKind::Function))
                    .union(KindSet::from_kind(ValueKind::Arguments)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TypedArrayPrototypeToString => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeJoin
            | StandardBuiltinId::ArrayPrototypeToLocaleString
            | StandardBuiltinId::TypedArrayPrototypeJoin
            | StandardBuiltinId::Uint8ArrayPrototypeToBase64
            | StandardBuiltinId::Uint8ArrayPrototypeToHex
            | StandardBuiltinId::TypedArrayPrototypeToLocaleString => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TypedArrayPrototypeFill
            | StandardBuiltinId::TypedArrayPrototypeReverse
            | StandardBuiltinId::TypedArrayPrototypeCopyWithin
            | StandardBuiltinId::TypedArrayPrototypeSort
            | StandardBuiltinId::TypedArrayPrototypeSubarray
            | StandardBuiltinId::TypedArrayPrototypeSlice
            | StandardBuiltinId::TypedArrayPrototypeToReversed
            | StandardBuiltinId::TypedArrayPrototypeToSorted
            | StandardBuiltinId::TypedArrayPrototypeWith => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TypedArrayPrototypeSet => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeFlat | StandardBuiltinId::ArrayPrototypeFlatMap => (
                ValueKind::Dynamic,
                Self::object_like_kind_set(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeAt | StandardBuiltinId::TypedArrayPrototypeAt => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeToReversed => (
                ValueKind::Array,
                KindSet::from_kind(ValueKind::Array),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeWith => (
                ValueKind::Array,
                KindSet::from_kind(ValueKind::Array),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeToSpliced => (
                ValueKind::Array,
                KindSet::from_kind(ValueKind::Array),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeToSorted => (
                ValueKind::Array,
                KindSet::from_kind(ValueKind::Array),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeReverse => (
                ValueKind::Dynamic,
                Self::object_like_kind_set(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeCopyWithin => (
                ValueKind::Dynamic,
                Self::object_like_kind_set(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeIncludes
            | StandardBuiltinId::TypedArrayPrototypeIncludes => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeIndexOf
            | StandardBuiltinId::TypedArrayPrototypeIndexOf => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeLastIndexOf
            | StandardBuiltinId::TypedArrayPrototypeLastIndexOf => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeFind => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TypedArrayPrototypeFind => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeFindIndex
            | StandardBuiltinId::TypedArrayPrototypeFindIndex => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeFindLast
            | StandardBuiltinId::TypedArrayPrototypeFindLast => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeFindLastIndex
            | StandardBuiltinId::TypedArrayPrototypeFindLastIndex => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeEvery
            | StandardBuiltinId::TypedArrayPrototypeEvery => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeSome | StandardBuiltinId::TypedArrayPrototypeSome => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TypedArrayPrototypeMap
            | StandardBuiltinId::TypedArrayPrototypeFilter => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeForEach
            | StandardBuiltinId::TypedArrayPrototypeForEach => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeFilter | StandardBuiltinId::ArrayPrototypeMap => (
                ValueKind::Dynamic,
                Self::object_like_kind_set(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeReduce
            | StandardBuiltinId::ArrayPrototypeReduceRight
            | StandardBuiltinId::TypedArrayPrototypeReduce
            | StandardBuiltinId::TypedArrayPrototypeReduceRight => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypePop => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeShift => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeFill | StandardBuiltinId::ArrayPrototypeSort => (
                ValueKind::Dynamic,
                Self::object_like_kind_set(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypePush => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeUnshift => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayPrototypeKeys
            | StandardBuiltinId::ArrayPrototypeEntries
            | StandardBuiltinId::ArrayPrototypeValues
            | StandardBuiltinId::TypedArrayPrototypeKeys
            | StandardBuiltinId::TypedArrayPrototypeEntries
            | StandardBuiltinId::TypedArrayPrototypeValues
            | StandardBuiltinId::StringPrototypeIterator => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                ValueInfo::undefined(),
            ),
            // The fourth member is the `constructor_instance`: the static type
            // of `new S()` when `S` is a class whose heritage reaches this
            // builtin and which declares no explicit constructor. A synthetic
            // derived constructor inherits it verbatim (see the
            // `inherited_instance` binding in `lower_class`), so spelling it
            // `ValueInfo::undefined()` here typed every instance of
            // `class S extends Iterator { ... }` as `undefined`.
            //
            // That was the root cause of the whole batch-5 `iterator_helpers`
            // failure set, measured rather than argued (b6 lane note
            // `iterator-helper-static-key-call-on-a-class-receiver`): with the
            // receiver typed nullish, `emit_method_call`'s statically-nullish
            // shortcut emitted *no call at all* for `find`/`reduce`/`take`/
            // `map`/`every`/`some`/`filter`, while the runtime value was an
            // ordinary object, so the emitted nullish test never fired and the
            // caller read stale scratch. `scriptw inspect` on
            // `class D extends Iterator {} new D();` printed
            // `result=undefined`, against `result=object` for every other
            // superclass — including a user-defined one.
            //
            // The prototype layered here is only what a *direct* construction
            // would see; the class path immediately overwrites it with the
            // subclass prototype, which already chains to `Iterator.prototype`
            // through `heritage_prototype`.
            StandardBuiltinId::IteratorConstructor => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorConstructor,
                )),
                Self::with_instance_prototype(
                    Self::fresh_constructed_instance_info(),
                    Some(Self::iterator_prototype_shape()),
                ),
            ),
            StandardBuiltinId::IteratorPrototypeToArray => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorPrototypeToArray,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeForEach => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorPrototypeForEach,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeIncludes => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorPrototypeIncludes,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeJoin => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorPrototypeJoin,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeEvery => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorPrototypeEvery,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeSome => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorPrototypeSome,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeFind => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorPrototypeFind,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeReduce => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorPrototypeReduce,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeMap => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorPrototypeMap,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorZipNext => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorZipNext,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorConcatNext => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorConcatNext,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorConcatReturn => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorConcatReturn,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorZipReturn => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorZipReturn,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorHelperNext => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorHelperNext,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorHelperReturn => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorHelperReturn,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorMapNext => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorMapNext,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorMapReturn => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorMapReturn,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeFilter => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorPrototypeFilter,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorFilterNext => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorFilterNext,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorFilterReturn => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorFilterReturn,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeFlatMap => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorPrototypeFlatMap,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorFlatMapNext => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorFlatMapNext,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorFlatMapReturn => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorFlatMapReturn,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeTake => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorPrototypeTake,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorTakeNext => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorTakeNext,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorTakeReturn => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorTakeReturn,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeChunks => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorPrototypeChunks,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeWindows => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorPrototypeWindows,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorBufferedNext => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorBufferedNext,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorBufferedReturn => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorBufferedReturn,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeDrop => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorPrototypeDrop,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorDropNext => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorDropNext,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorDropReturn => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorDropReturn,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeConstructorGetter => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                Some(Self::standard_builtin_function_shape(
                    StandardBuiltinId::IteratorConstructor,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeConstructorSetter => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeSymbolDispose => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeToStringTagGetter => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorPrototypeToStringTagSetter => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorFromWrapperNext => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Box::new(Self::empty_object_shape())),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IteratorFromWrapperReturn => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Box::new(Self::empty_object_shape())),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayIteratorNext
            | StandardBuiltinId::StringIteratorNext
            | StandardBuiltinId::RegExpStringIteratorNext
            | StandardBuiltinId::GeneratorPrototypeNext
            | StandardBuiltinId::GeneratorPrototypeReturn
            | StandardBuiltinId::GeneratorPrototypeThrow => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Box::new(Self::empty_object_shape())),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::AsyncGeneratorPrototypeNext
            | StandardBuiltinId::AsyncGeneratorPrototypeReturn
            | StandardBuiltinId::AsyncGeneratorPrototypeThrow
            | StandardBuiltinId::AsyncIteratorPrototypeAsyncDispose => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::promise_instance_shape()),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::AsyncIteratorPrototypeAsyncDisposeFulfilled => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayIteratorIdentity | StandardBuiltinId::AsyncIteratorIdentity => {
                (
                    ValueKind::Object,
                    KindSet::from_kind(ValueKind::Object),
                    Some(Box::new(Self::empty_object_shape())),
                    ValueInfo::undefined(),
                )
            }
            StandardBuiltinId::ArrayBufferConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::array_buffer_instance_shape()),
                Self::value_info_from_shape(Some(Self::array_buffer_instance_shape())),
            ),
            StandardBuiltinId::SharedArrayBufferConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::shared_array_buffer_instance_shape()),
                Self::value_info_from_shape(Some(Self::shared_array_buffer_instance_shape())),
            ),
            StandardBuiltinId::ArrayBufferIsView => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArraySpeciesGetter
            | StandardBuiltinId::TypedArraySpeciesGetter
            | StandardBuiltinId::ArrayBufferSpeciesGetter
            | StandardBuiltinId::RegExpSpeciesGetter => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::RegExpPrototypeFlagsGetter => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::RegExpPrototypeSourceGetter => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::RegExpPrototypeHasIndicesGetter
            | StandardBuiltinId::RegExpPrototypeGlobalGetter
            | StandardBuiltinId::RegExpPrototypeIgnoreCaseGetter
            | StandardBuiltinId::RegExpPrototypeMultilineGetter
            | StandardBuiltinId::RegExpPrototypeDotAllGetter
            | StandardBuiltinId::RegExpPrototypeUnicodeGetter
            | StandardBuiltinId::RegExpPrototypeUnicodeSetsGetter
            | StandardBuiltinId::RegExpPrototypeStickyGetter => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayBufferPrototypeByteLengthGetter => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SharedArrayBufferPrototypeByteLengthGetter => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SharedArrayBufferPrototypeMaxByteLengthGetter => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SharedArrayBufferPrototypeGrowableGetter => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SharedArrayBufferPrototypeGrow => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayBufferPrototypeMaxByteLengthGetter => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayBufferPrototypeDetachedGetter => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayBufferPrototypeResizableGetter
            | StandardBuiltinId::ArrayBufferPrototypeImmutableGetter => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayBufferPrototypeResize => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayBufferPrototypeSlice => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::array_buffer_instance_shape()),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SharedArrayBufferPrototypeSlice => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::shared_array_buffer_instance_shape()),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ArrayBufferPrototypeTransfer
            | StandardBuiltinId::ArrayBufferPrototypeTransferToFixedLength
            | StandardBuiltinId::ArrayBufferPrototypeTransferToImmutable
            | StandardBuiltinId::ArrayBufferPrototypeSliceToImmutable => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::array_buffer_instance_shape()),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::DataViewPrototypeBufferGetter => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::array_buffer_instance_shape()),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TypedArrayPrototypeBufferGetter => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::array_buffer_instance_shape()),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::DataViewPrototypeByteLengthGetter
            | StandardBuiltinId::DataViewPrototypeByteOffsetGetter
            | StandardBuiltinId::TypedArrayPrototypeByteLengthGetter
            | StandardBuiltinId::TypedArrayPrototypeByteOffsetGetter
            | StandardBuiltinId::TypedArrayPrototypeLengthGetter => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TypedArrayPrototypeToStringTagGetter => (
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::Undefined)
                    .union(KindSet::from_kind(ValueKind::String)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TypedArrayFrom | StandardBuiltinId::TypedArrayOf => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::Uint8ArrayFromBase64 | StandardBuiltinId::Uint8ArrayFromHex => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::typed_array_instance_shape_for_constructor(
                    StandardBuiltinId::Uint8ArrayConstructor,
                )),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::Uint8ArrayPrototypeSetFromBase64
            | StandardBuiltinId::Uint8ArrayPrototypeSetFromHex => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::uint8_array_codec_result_shape()),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::DataViewConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::data_view_instance_shape()),
                Self::value_info_from_shape(Some(Self::data_view_instance_shape())),
            ),
            StandardBuiltinId::DateConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::date_instance_shape()),
                Self::value_info_from_shape(Some(Self::date_instance_shape())),
            ),
            StandardBuiltinId::TemporalNowTimeZoneId => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalNowInstant => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_instant_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_instant_instance_shape())),
            ),
            StandardBuiltinId::TemporalNowZonedDateTimeIso => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_zoned_date_time_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_zoned_date_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalNowPlainDateTimeIso => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_date_time_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_plain_date_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalNowPlainDateIso => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_date_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_plain_date_instance_shape())),
            ),
            StandardBuiltinId::TemporalNowPlainTimeIso => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_time_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_plain_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalInstantConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_instant_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_instant_instance_shape())),
            ),
            StandardBuiltinId::TemporalInstantFrom
            | StandardBuiltinId::TemporalInstantFromEpochMilliseconds
            | StandardBuiltinId::TemporalInstantPrototypeAdd
            | StandardBuiltinId::TemporalInstantPrototypeSubtract
            | StandardBuiltinId::TemporalInstantPrototypeRound
            | StandardBuiltinId::TemporalInstantFromEpochNanoseconds => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_instant_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_instant_instance_shape())),
            ),
            StandardBuiltinId::TemporalInstantCompare => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalInstantPrototypeEpochMillisecondsGetter => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalInstantPrototypeEpochNanosecondsGetter => (
                ValueKind::BigInt,
                KindSet::from_kind(ValueKind::BigInt),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalInstantPrototypeEquals => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalInstantPrototypeToString
            | StandardBuiltinId::TemporalInstantPrototypeToLocaleString
            | StandardBuiltinId::TemporalInstantPrototypeToJson => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            // `valueOf` always throws, so the only completion that reaches a
            // caller is a throw; the normal-completion kind is unreachable and
            // is spelled `Undefined` the same way
            // `TemporalDurationPrototypeValueOf` spells it.
            StandardBuiltinId::TemporalInstantPrototypeValueOf => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IntlLocalePrototypeGetTimeZones => (
                ValueKind::Array,
                KindSet::from_kind(ValueKind::Array)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IntlSupportedValuesOf
            | StandardBuiltinId::IntlLocalePrototypeGetCalendars
            | StandardBuiltinId::IntlLocalePrototypeGetCollations
            | StandardBuiltinId::IntlLocalePrototypeGetNumberingSystems
            | StandardBuiltinId::IntlLocalePrototypeGetHourCycles
            | StandardBuiltinId::IntlGetCanonicalLocales => (
                ValueKind::Array,
                KindSet::from_kind(ValueKind::Array),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IntlLocaleConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::intl_locale_instance_shape()),
                Self::value_info_from_shape(Some(Self::intl_locale_instance_shape())),
            ),
            StandardBuiltinId::IntlLocalePrototypeGetTextInfo
            | StandardBuiltinId::IntlLocalePrototypeGetWeekInfo => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IntlLocalePrototypeMaximize
            | StandardBuiltinId::IntlLocalePrototypeMinimize => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::intl_locale_instance_shape()),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IntlLocalePrototypeLanguageGetter
            | StandardBuiltinId::IntlLocalePrototypeBaseNameGetter
            | StandardBuiltinId::IntlLocalePrototypeToString => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IntlLocalePrototypeNumericGetter => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IntlLocalePrototypeScriptGetter
            | StandardBuiltinId::IntlLocalePrototypeRegionGetter
            | StandardBuiltinId::IntlLocalePrototypeCalendarGetter
            | StandardBuiltinId::IntlLocalePrototypeCollationGetter
            | StandardBuiltinId::IntlLocalePrototypeFirstDayOfWeekGetter
            | StandardBuiltinId::IntlLocalePrototypeHourCycleGetter
            | StandardBuiltinId::IntlLocalePrototypeCaseFirstGetter
            | StandardBuiltinId::IntlLocalePrototypeNumberingSystemGetter
            | StandardBuiltinId::IntlLocalePrototypeVariantsGetter => (
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::String)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                None,
                ValueInfo::undefined(),
            ),
            // Split out of the `resolvedOptions` or-pattern deliberately: only
            // one of the two is constructable (`builtins.rs`'s
            // `constructable()` lists the constructor and not the accessor), so
            // only one of them can ever reach the `constructor_instance`
            // consumer. Giving `resolvedOptions` a constructor instance would
            // be meaningless and would hide the arm that matters.
            //
            // `class S extends Intl.DateTimeFormat {}` with no explicit
            // constructor takes its static instance type from this fourth tuple
            // member. Spelling it `ValueInfo::undefined()` typed every such
            // instance as nullish, which is the batch-5 `IteratorConstructor`
            // defect verbatim; batch 6 fixed that arm and left this one, which
            // the b6 lane note filed after intersecting all 332
            // `standard_builtin_signature` arms with the 53 `constructable()`
            // ids. Four carried `ValueInfo::undefined()`; this is the only
            // reachable one (`Proxy.prototype` is `undefined` so the `extends`
            // itself throws, and `new Symbol()` / `new BigInt()` throw before an
            // instance exists).
            //
            // MEASURED before the change, not deduced:
            //
            //   scriptw inspect `class D extends Intl.DateTimeFormat {} new D();`
            //     -> result=undefined
            //   scriptw inspect `class D extends Intl.Locale {} new D("en");`
            //     -> result=object
            //
            // and the consequence, `scriptw run --execution-backend wasm` over
            // three classes differing only in heritage, each declaring its own
            // `reduceRight(a) { return a + 1; }`:
            //
            //   dtf.reduceRight=undefined   loc.reduceRight=2   plain.reduceRight=2
            //
            // A plain user method was not called at all. `reduceRight` is the
            // one of the ten names `lowering.rs` builds an `ExprIr::CallMethod`
            // for on a non-array receiver that has no `IteratorHelper` variant,
            // so it is the only one still reaching `emit_method_call`'s
            // statically-nullish shortcut after batch 6 widened
            // `receiver_needs_dynamic_helper_dispatch`. Fixture:
            // `crates/scriptw-cli/tests/fixtures/wasm_intl_date_time_format_subclass.js`.
            //
            // THE SECOND CONSUMER, which the lane spec and its note both missed
            // by calling `inherited_instance` in `lower_class` the only one.
            // `lower_new_expression` also reads `signature.constructor_instance`
            // — the `else` arm of `null_heritage_return_path` — and
            // `standard_builtin_signature` hard-codes `class_heritage_kind:
            // ClassHeritageKind::None`, so that flag is false here and the arm
            // IS taken for a *direct* `new Intl.DateTimeFormat("en")`, not only
            // for a subclass. That path changes from `{Undefined, Object}` to
            // `{Object}`, and the pre-merge value (Object plus
            // `empty_object_shape()`) is what `merge_function_this_info`
            // receives.
            //
            // Traced, and benign: `merge_value_infos` takes its equal-kind
            // branch and `merge_heap_shapes` answers `None` when only one side
            // carries a shape, so the empty shape never escapes and the
            // narrowing is a strict improvement. The `merge_function_this_info`
            // argument is the one edge that was not separately measured. The
            // fixture covers this path directly as well as through the
            // subclass, so the `date::` chunk exercises both.
            //
            // `fresh_constructed_instance_info()` rather than a layered
            // prototype: there is no `intl_date_time_format_*_shape` in this
            // file, DTF's own `return_shape` is `None`, and the
            // `IteratorConstructor` comment above records that `lower_class`
            // immediately overwrites any layered prototype with the subclass
            // prototype. The `Intl.Locale` precedent
            // (`value_info_from_shape(Some(intl_locale_instance_shape()))`) is
            // unavailable because `Locale` HAS an instance shape and this does
            // not.
            StandardBuiltinId::IntlDateTimeFormatConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::IntlDateTimeFormatPrototypeResolvedOptions => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IntlDateTimeFormatSupportedLocalesOf
            | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatToParts
            | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRangeToParts => (
                ValueKind::Array,
                KindSet::from_kind(ValueKind::Array),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IntlDateTimeFormatPrototypeFormatGetter => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IntlDateTimeFormatBoundFormat
            | StandardBuiltinId::IntlDateTimeFormatPrototypeFormatRange => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IntlNumberFormatConstructor
            | StandardBuiltinId::IntlPluralRulesConstructor
            | StandardBuiltinId::IntlListFormatConstructor
            | StandardBuiltinId::IntlCollatorConstructor
            | StandardBuiltinId::IntlDisplayNamesConstructor
            | StandardBuiltinId::IntlRelativeTimeFormatConstructor
            | StandardBuiltinId::IntlDurationFormatConstructor
            | StandardBuiltinId::IntlSegmenterConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::IntlNumberFormatPrototypeResolvedOptions
            | StandardBuiltinId::IntlPluralRulesPrototypeResolvedOptions
            | StandardBuiltinId::IntlListFormatPrototypeResolvedOptions
            | StandardBuiltinId::IntlCollatorPrototypeResolvedOptions
            | StandardBuiltinId::IntlDisplayNamesPrototypeResolvedOptions
            | StandardBuiltinId::IntlRelativeTimeFormatPrototypeResolvedOptions
            | StandardBuiltinId::IntlDurationFormatPrototypeResolvedOptions
            | StandardBuiltinId::IntlSegmenterPrototypeSegment
            | StandardBuiltinId::IntlSegmenterPrototypeResolvedOptions
            | StandardBuiltinId::IntlSegmentsPrototypeIterator
            | StandardBuiltinId::IntlSegmentIteratorPrototypeNext => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IntlNumberFormatSupportedLocalesOf
            | StandardBuiltinId::IntlPluralRulesSupportedLocalesOf
            | StandardBuiltinId::IntlListFormatSupportedLocalesOf
            | StandardBuiltinId::IntlCollatorSupportedLocalesOf
            | StandardBuiltinId::IntlDisplayNamesSupportedLocalesOf
            | StandardBuiltinId::IntlRelativeTimeFormatSupportedLocalesOf
            | StandardBuiltinId::IntlDurationFormatSupportedLocalesOf
            | StandardBuiltinId::IntlSegmenterSupportedLocalesOf
            | StandardBuiltinId::IntlListFormatPrototypeFormatToParts
            | StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormatToParts
            | StandardBuiltinId::IntlDurationFormatPrototypeFormatToParts
            | StandardBuiltinId::IntlNumberFormatPrototypeFormatToParts
            | StandardBuiltinId::IntlNumberFormatPrototypeFormatRangeToParts => (
                ValueKind::Array,
                KindSet::from_kind(ValueKind::Array),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IntlNumberFormatPrototypeFormatGetter
            | StandardBuiltinId::IntlCollatorPrototypeCompareGetter => (
                ValueKind::Function,
                KindSet::from_kind(ValueKind::Function),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::IntlNumberFormatBoundFormat
            | StandardBuiltinId::IntlNumberFormatPrototypeFormatRange
            | StandardBuiltinId::IntlPluralRulesPrototypeSelect
            | StandardBuiltinId::IntlPluralRulesPrototypeSelectRange
            | StandardBuiltinId::IntlListFormatPrototypeFormat
            | StandardBuiltinId::IntlRelativeTimeFormatPrototypeFormat
            | StandardBuiltinId::IntlDurationFormatPrototypeFormat => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainDateTimeConstructor
            | StandardBuiltinId::TemporalPlainDateTimeFrom
            | StandardBuiltinId::TemporalPlainDateTimePrototypeWith
            | StandardBuiltinId::TemporalPlainDateTimePrototypeWithPlainTime
            | StandardBuiltinId::TemporalPlainDateTimePrototypeWithCalendar
            | StandardBuiltinId::TemporalPlainDateTimePrototypeAdd
            | StandardBuiltinId::TemporalPlainDateTimePrototypeSubtract
            | StandardBuiltinId::TemporalPlainDateTimePrototypeRound => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_date_time_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_plain_date_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDateTimePrototypeUntil
            | StandardBuiltinId::TemporalPlainDateTimePrototypeSince => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_duration_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_duration_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainDate => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_date_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_plain_date_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDateTimePrototypeToPlainTime => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_time_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_plain_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDateTimePrototypeToZonedDateTime => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_zoned_date_time_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_zoned_date_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDateTimeCompare
            | StandardBuiltinId::TemporalPlainDateTimePrototypeYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMonthGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDayGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeHourGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMinuteGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeSecondGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMillisecondGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMicrosecondGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeNanosecondGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfWeekGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDayOfYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeWeekOfYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeYearOfWeekGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInWeekGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInMonthGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeDaysInYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMonthsInYearGetter => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainDateTimePrototypeCalendarIdGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeMonthCodeGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeToString
            | StandardBuiltinId::TemporalPlainDateTimePrototypeToJson
            | StandardBuiltinId::TemporalPlainDateTimePrototypeToLocaleString => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainDateTimePrototypeInLeapYearGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeEquals => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainDateTimePrototypeEraGetter
            | StandardBuiltinId::TemporalPlainDateTimePrototypeValueOf => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainDateTimePrototypeEraYearGetter => (
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::Number)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainDateConstructor
            | StandardBuiltinId::TemporalPlainDateFrom
            | StandardBuiltinId::TemporalPlainDatePrototypeWith
            | StandardBuiltinId::TemporalPlainDatePrototypeWithCalendar
            | StandardBuiltinId::TemporalPlainDatePrototypeAdd
            | StandardBuiltinId::TemporalPlainDatePrototypeSubtract => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_date_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_plain_date_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDatePrototypeUntil
            | StandardBuiltinId::TemporalPlainDatePrototypeSince => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_duration_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_duration_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDatePrototypeToPlainDateTime => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_date_time_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_plain_date_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDatePrototypeToPlainYearMonth => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_year_month_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_plain_year_month_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainDatePrototypeToPlainMonthDay => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_month_day_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_plain_month_day_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainYearMonthConstructor
            | StandardBuiltinId::TemporalPlainYearMonthFrom
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeWith
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeAdd
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeSubtract => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_year_month_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_plain_year_month_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainYearMonthPrototypeUntil
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeSince => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_duration_instance_shape()),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainYearMonthPrototypeToPlainDate
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeToPlainDate => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_date_instance_shape()),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainMonthDayConstructor
            | StandardBuiltinId::TemporalPlainMonthDayFrom
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeWith => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_month_day_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_plain_month_day_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainYearMonthCompare
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeYearGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInYearGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeDaysInMonthGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthsInYearGetter
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeDayGetter => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainYearMonthPrototypeCalendarIdGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeMonthCodeGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeToString
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeToJson
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeToLocaleString
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeCalendarIdGetter
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeMonthCodeGetter
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeToString
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeToJson
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeToLocaleString => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainYearMonthPrototypeInLeapYearGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeEquals
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeEquals => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainYearMonthPrototypeEraGetter
            | StandardBuiltinId::TemporalPlainYearMonthPrototypeValueOf
            | StandardBuiltinId::TemporalPlainMonthDayPrototypeValueOf => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainYearMonthPrototypeEraYearGetter => (
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::Number)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalDurationConstructor
            | StandardBuiltinId::TemporalDurationFrom
            | StandardBuiltinId::TemporalDurationPrototypeWith
            | StandardBuiltinId::TemporalDurationPrototypeNegated
            | StandardBuiltinId::TemporalDurationPrototypeAbs
            | StandardBuiltinId::TemporalDurationPrototypeAdd
            | StandardBuiltinId::TemporalDurationPrototypeSubtract
            | StandardBuiltinId::TemporalDurationPrototypeRound => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_duration_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_duration_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainTimeConstructor
            | StandardBuiltinId::TemporalPlainTimeFrom
            | StandardBuiltinId::TemporalPlainTimePrototypeWith
            | StandardBuiltinId::TemporalPlainTimePrototypeAdd
            | StandardBuiltinId::TemporalPlainTimePrototypeSubtract
            | StandardBuiltinId::TemporalPlainTimePrototypeRound => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_time_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_plain_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalInstantPrototypeUntil
            | StandardBuiltinId::TemporalInstantPrototypeSince
            | StandardBuiltinId::TemporalPlainTimePrototypeUntil
            | StandardBuiltinId::TemporalPlainTimePrototypeSince => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_duration_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_duration_instance_shape())),
            ),
            StandardBuiltinId::TemporalPlainTimeCompare
            | StandardBuiltinId::TemporalPlainTimePrototypeHourGetter
            | StandardBuiltinId::TemporalPlainTimePrototypeMinuteGetter
            | StandardBuiltinId::TemporalPlainTimePrototypeSecondGetter
            | StandardBuiltinId::TemporalPlainTimePrototypeMillisecondGetter
            | StandardBuiltinId::TemporalPlainTimePrototypeMicrosecondGetter
            | StandardBuiltinId::TemporalPlainTimePrototypeNanosecondGetter => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainTimePrototypeToString
            | StandardBuiltinId::TemporalPlainTimePrototypeToJson
            | StandardBuiltinId::TemporalPlainTimePrototypeToLocaleString => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainTimePrototypeEquals => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            // `valueOf` always throws a TypeError, so it has no return kind that
            // any caller can observe.
            StandardBuiltinId::TemporalPlainTimePrototypeValueOf => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalDurationCompare
            | StandardBuiltinId::TemporalDurationPrototypeYearsGetter
            | StandardBuiltinId::TemporalDurationPrototypeMonthsGetter
            | StandardBuiltinId::TemporalDurationPrototypeWeeksGetter
            | StandardBuiltinId::TemporalDurationPrototypeDaysGetter
            | StandardBuiltinId::TemporalDurationPrototypeHoursGetter
            | StandardBuiltinId::TemporalDurationPrototypeMinutesGetter
            | StandardBuiltinId::TemporalDurationPrototypeSecondsGetter
            | StandardBuiltinId::TemporalDurationPrototypeMillisecondsGetter
            | StandardBuiltinId::TemporalDurationPrototypeMicrosecondsGetter
            | StandardBuiltinId::TemporalDurationPrototypeNanosecondsGetter
            | StandardBuiltinId::TemporalDurationPrototypeSignGetter
            | StandardBuiltinId::TemporalDurationPrototypeTotal => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalDurationPrototypeToString
            | StandardBuiltinId::TemporalDurationPrototypeToJson
            | StandardBuiltinId::TemporalDurationPrototypeToLocaleString => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalDurationPrototypeBlankGetter => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalDurationPrototypeValueOf => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainDateCompare
            | StandardBuiltinId::TemporalPlainDatePrototypeYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeMonthGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDayGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDayOfWeekGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDayOfYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeWeekOfYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeYearOfWeekGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDaysInWeekGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDaysInMonthGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeDaysInYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeMonthsInYearGetter => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainDatePrototypeCalendarIdGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeMonthCodeGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeToString
            | StandardBuiltinId::TemporalPlainDatePrototypeToJson
            | StandardBuiltinId::TemporalPlainDatePrototypeToLocaleString => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainDatePrototypeInLeapYearGetter
            | StandardBuiltinId::TemporalPlainDatePrototypeEquals => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            // `era`/`eraYear` are `undefined` in the ISO 8601 calendar, and ISO
            // 8601 is the only calendar this backend accepts.
            StandardBuiltinId::TemporalPlainDatePrototypeEraGetter => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalPlainDatePrototypeEraYearGetter => (
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::Number)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                None,
                ValueInfo::undefined(),
            ),
            // `valueOf` always throws a TypeError, so it has no return kind that
            // any caller can observe.
            StandardBuiltinId::TemporalPlainDatePrototypeValueOf => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalZonedDateTimePrototypeGetTimeZoneTransition => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object).union(KindSet::from_kind(ValueKind::Null)),
                Some(Self::temporal_zoned_date_time_instance_shape()),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalZonedDateTimeConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_zoned_date_time_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_zoned_date_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDate => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_date_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_plain_date_instance_shape())),
            ),
            StandardBuiltinId::TemporalZonedDateTimePrototypeWith
            | StandardBuiltinId::TemporalZonedDateTimePrototypeWithPlainTime
            | StandardBuiltinId::TemporalZonedDateTimePrototypeRound
            | StandardBuiltinId::TemporalZonedDateTimePrototypeStartOfDay
            | StandardBuiltinId::TemporalPlainDatePrototypeToZonedDateTime
            | StandardBuiltinId::TemporalInstantPrototypeToZonedDateTimeIso
            | StandardBuiltinId::TemporalZonedDateTimeFrom => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_zoned_date_time_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_zoned_date_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalZonedDateTimePrototypeEpochMillisecondsGetter => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalZonedDateTimePrototypeEpochNanosecondsGetter => (
                ValueKind::BigInt,
                KindSet::from_kind(ValueKind::BigInt),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetNanosecondsGetter => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalZonedDateTimePrototypeOffsetGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeTimeZoneIdGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeCalendarIdGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMonthCodeGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeToString
            | StandardBuiltinId::TemporalZonedDateTimePrototypeToJson
            | StandardBuiltinId::TemporalZonedDateTimePrototypeToLocaleString => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            // `valueOf` always throws, so the only completion that reaches a
            // caller is a throw; the normal-completion kind is unreachable and
            // is spelled `Undefined` the same way
            // `TemporalInstantPrototypeValueOf` spells it.
            StandardBuiltinId::TemporalZonedDateTimePrototypeValueOf => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainTime => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_time_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_plain_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalZonedDateTimeCompare
            | StandardBuiltinId::TemporalZonedDateTimePrototypeHoursInDayGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfWeekGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDayOfYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInWeekGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInMonthGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDaysInYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMonthsInYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMonthGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeDayGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeHourGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMinuteGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeSecondGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMillisecondGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeMicrosecondGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeNanosecondGetter => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            // Declared exactly as the PlainDate / PlainDateTime / PlainYearMonth
            // era pair already is, so the four cannot disagree. `era` is filed
            // under `Undefined` and `eraYear` under `Dynamic`; that asymmetry
            // is inherited, not invented here.
            //
            // `Undefined` is a *wrong* declaration for a two-calendar backend
            // — a gregory receiver answers the string `"ce"` — and it is
            // harmless only because nothing consults it on this path. Every
            // singleton-`possible_kinds` shortcut in the backend is gated on
            // `planning::expr_result_tag_is_runtime_dynamic`, whose or-pattern
            // returns `true` for `PropertyRead`, `CallNamed` and `CallMethod`;
            // those are the only syntactic forms that can reach an era getter,
            // so the declared kind never decides the emitted tag. (Do not cite
            // the `intl402/Temporal/PlainDate` 488/488 node for this, as an
            // earlier version of this comment did: that snapshot is
            // `spec-exec`, and these tables are read by wasm-aot lowering
            // only.)
            //
            // The correct fix is to widen all four `era` getters to
            // `ValueKind::Dynamic` with `String|Undefined`, mirroring what the
            // four `eraYear` getters already do with `Number|Undefined`, in
            // one edit so they cannot disagree.
            StandardBuiltinId::TemporalZonedDateTimePrototypeEraGetter => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalZonedDateTimePrototypeWeekOfYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeYearOfWeekGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeEraYearGetter => (
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::Number)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalZonedDateTimePrototypeInLeapYearGetter
            | StandardBuiltinId::TemporalZonedDateTimePrototypeEquals => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TemporalZonedDateTimePrototypeToInstant => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_instant_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_instant_instance_shape())),
            ),
            // `withTimeZone`, `withCalendar`, `add` and `subtract` are all
            // ZonedDateTime-in / ZonedDateTime-out; `until`/`since` hand back a
            // `Temporal.Duration`, exactly as the PlainDateTime arms above
            // already declare for their namesakes.
            StandardBuiltinId::TemporalZonedDateTimePrototypeWithTimeZone
            | StandardBuiltinId::TemporalZonedDateTimePrototypeWithCalendar
            | StandardBuiltinId::TemporalZonedDateTimePrototypeAdd
            | StandardBuiltinId::TemporalZonedDateTimePrototypeSubtract => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_zoned_date_time_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_zoned_date_time_instance_shape())),
            ),
            StandardBuiltinId::TemporalZonedDateTimePrototypeUntil
            | StandardBuiltinId::TemporalZonedDateTimePrototypeSince => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_duration_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_duration_instance_shape())),
            ),
            StandardBuiltinId::TemporalZonedDateTimePrototypeToPlainDateTime => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::temporal_plain_date_time_instance_shape()),
                Self::value_info_from_shape(Some(Self::temporal_plain_date_time_instance_shape())),
            ),
            StandardBuiltinId::RegExpConstructor => (
                // Ordinary Call can return its object argument unchanged,
                // including an Array or Function whose @@match is truthy.
                ValueKind::Dynamic,
                KindSet::HEAP_COERCIBLE_ONLY.union(KindSet::from_kind(ValueKind::Function)),
                None,
                Self::value_info_from_shape(Some(Self::regexp_instance_shape())),
            ),
            StandardBuiltinId::RegExpLegacyStaticGetter => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::RegExpLegacyStaticSetter => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::RegExpEscape => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::DateNow
            | StandardBuiltinId::DateParse
            | StandardBuiltinId::DateUtc
            | StandardBuiltinId::DatePrototypeGetTime
            | StandardBuiltinId::DatePrototypeSetTime
            | StandardBuiltinId::DatePrototypeValueOf
            | StandardBuiltinId::DatePrototypeGetFullYear
            | StandardBuiltinId::DatePrototypeGetUtcFullYear
            | StandardBuiltinId::DatePrototypeGetMonth
            | StandardBuiltinId::DatePrototypeGetUtcMonth
            | StandardBuiltinId::DatePrototypeGetDate
            | StandardBuiltinId::DatePrototypeGetUtcDate
            | StandardBuiltinId::DatePrototypeGetDay
            | StandardBuiltinId::DatePrototypeGetUtcDay
            | StandardBuiltinId::DatePrototypeGetHours
            | StandardBuiltinId::DatePrototypeGetUtcHours
            | StandardBuiltinId::DatePrototypeGetMinutes
            | StandardBuiltinId::DatePrototypeGetUtcMinutes
            | StandardBuiltinId::DatePrototypeGetSeconds
            | StandardBuiltinId::DatePrototypeGetUtcSeconds
            | StandardBuiltinId::DatePrototypeGetMilliseconds
            | StandardBuiltinId::DatePrototypeGetUtcMilliseconds
            | StandardBuiltinId::DatePrototypeGetTimezoneOffset
            | StandardBuiltinId::DatePrototypeGetYear
            | StandardBuiltinId::DatePrototypeSetYear
            | StandardBuiltinId::DatePrototypeSetFullYear
            | StandardBuiltinId::DatePrototypeSetUtcFullYear
            | StandardBuiltinId::DatePrototypeSetMonth
            | StandardBuiltinId::DatePrototypeSetUtcMonth
            | StandardBuiltinId::DatePrototypeSetDate
            | StandardBuiltinId::DatePrototypeSetUtcDate
            | StandardBuiltinId::DatePrototypeSetHours
            | StandardBuiltinId::DatePrototypeSetUtcHours
            | StandardBuiltinId::DatePrototypeSetMinutes
            | StandardBuiltinId::DatePrototypeSetUtcMinutes
            | StandardBuiltinId::DatePrototypeSetSeconds
            | StandardBuiltinId::DatePrototypeSetUtcSeconds
            | StandardBuiltinId::DatePrototypeSetMilliseconds
            | StandardBuiltinId::DatePrototypeSetUtcMilliseconds => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::DatePrototypeToJson => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::DatePrototypeToPrimitive => (
                ValueKind::Dynamic,
                KindSet::PRIMITIVE_ONLY,
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::DatePrototypeToTemporalInstant => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::DatePrototypeToIsoString
            | StandardBuiltinId::DatePrototypeToDateString
            | StandardBuiltinId::DatePrototypeToLocaleDateString
            | StandardBuiltinId::DatePrototypeToLocaleString
            | StandardBuiltinId::DatePrototypeToLocaleTimeString
            | StandardBuiltinId::DatePrototypeToTimeString
            | StandardBuiltinId::DatePrototypeToString
            | StandardBuiltinId::DatePrototypeToUtcString => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::Float64ArrayConstructor
            | StandardBuiltinId::Float32ArrayConstructor
            | StandardBuiltinId::Float16ArrayConstructor
            | StandardBuiltinId::Int32ArrayConstructor
            | StandardBuiltinId::Int16ArrayConstructor
            | StandardBuiltinId::Int8ArrayConstructor
            | StandardBuiltinId::Uint32ArrayConstructor
            | StandardBuiltinId::Uint16ArrayConstructor
            | StandardBuiltinId::Uint8ArrayConstructor
            | StandardBuiltinId::Uint8ClampedArrayConstructor
            | StandardBuiltinId::BigInt64ArrayConstructor
            | StandardBuiltinId::BigUint64ArrayConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::typed_array_instance_shape()),
                Self::value_info_from_shape(Some(Self::typed_array_instance_shape())),
            ),
            StandardBuiltinId::DataViewPrototypeGetUint8
            | StandardBuiltinId::DataViewPrototypeGetInt8
            | StandardBuiltinId::DataViewPrototypeGetUint16
            | StandardBuiltinId::DataViewPrototypeGetInt16
            | StandardBuiltinId::DataViewPrototypeGetUint32
            | StandardBuiltinId::DataViewPrototypeGetInt32
            | StandardBuiltinId::DataViewPrototypeGetFloat16
            | StandardBuiltinId::DataViewPrototypeGetFloat32
            | StandardBuiltinId::DataViewPrototypeGetFloat64 => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::DataViewPrototypeGetBigInt64
            | StandardBuiltinId::DataViewPrototypeGetBigUint64 => (
                ValueKind::BigInt,
                KindSet::from_kind(ValueKind::BigInt),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::DataViewPrototypeSetUint8
            | StandardBuiltinId::DataViewPrototypeSetInt8
            | StandardBuiltinId::DataViewPrototypeSetUint16
            | StandardBuiltinId::DataViewPrototypeSetInt16
            | StandardBuiltinId::DataViewPrototypeSetUint32
            | StandardBuiltinId::DataViewPrototypeSetInt32
            | StandardBuiltinId::DataViewPrototypeSetFloat16
            | StandardBuiltinId::DataViewPrototypeSetFloat32
            | StandardBuiltinId::DataViewPrototypeSetFloat64
            | StandardBuiltinId::DataViewPrototypeSetBigInt64
            | StandardBuiltinId::DataViewPrototypeSetBigUint64 => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::BigIntConstructor => (
                ValueKind::BigInt,
                KindSet::from_kind(ValueKind::BigInt),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::BigIntAsIntN | StandardBuiltinId::BigIntAsUintN => (
                ValueKind::BigInt,
                KindSet::from_kind(ValueKind::BigInt),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::BigIntPrototypeToString
            | StandardBuiltinId::BigIntPrototypeToLocaleString => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::BigIntPrototypeValueOf => (
                ValueKind::BigInt,
                KindSet::from_kind(ValueKind::BigInt),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::NumberConstructor => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                Self::boxed_primitive_instance_info(ValueInfo::new(ValueKind::Number)),
            ),
            StandardBuiltinId::StringConstructor => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                Self::boxed_primitive_instance_info(ValueInfo::new(ValueKind::String)),
            ),
            StandardBuiltinId::StringFromCharCode => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::StringFromCodePoint => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::StringRaw => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::StringPrototypeToString
            | StandardBuiltinId::StringPrototypeValueOf
            | StandardBuiltinId::StringPrototypeCharAt
            | StandardBuiltinId::StringPrototypeConcat
            | StandardBuiltinId::StringPrototypeAnchor
            | StandardBuiltinId::StringPrototypeBig
            | StandardBuiltinId::StringPrototypeBlink
            | StandardBuiltinId::StringPrototypeBold
            | StandardBuiltinId::StringPrototypeFixed
            | StandardBuiltinId::StringPrototypeFontcolor
            | StandardBuiltinId::StringPrototypeFontsize
            | StandardBuiltinId::StringPrototypeItalics
            | StandardBuiltinId::StringPrototypeLink
            | StandardBuiltinId::StringPrototypeSmall
            | StandardBuiltinId::StringPrototypeStrike
            | StandardBuiltinId::StringPrototypeSub
            | StandardBuiltinId::StringPrototypeSubstr
            | StandardBuiltinId::StringPrototypeSubstring
            | StandardBuiltinId::StringPrototypeSup
            | StandardBuiltinId::StringPrototypeSlice
            | StandardBuiltinId::StringPrototypePadStart
            | StandardBuiltinId::StringPrototypePadEnd
            | StandardBuiltinId::StringPrototypeRepeat
            | StandardBuiltinId::StringPrototypeNormalize
            | StandardBuiltinId::StringPrototypeToLocaleLowerCase
            | StandardBuiltinId::StringPrototypeToLocaleUpperCase
            | StandardBuiltinId::StringPrototypeToLowerCase
            | StandardBuiltinId::StringPrototypeToUpperCase
            | StandardBuiltinId::StringPrototypeTrim
            | StandardBuiltinId::StringPrototypeTrimStart
            | StandardBuiltinId::StringPrototypeTrimEnd
            | StandardBuiltinId::StringPrototypeToWellFormed
            | StandardBuiltinId::Escape
            | StandardBuiltinId::Unescape
            | StandardBuiltinId::EncodeUri
            | StandardBuiltinId::EncodeUriComponent
            | StandardBuiltinId::DecodeUri
            | StandardBuiltinId::DecodeUriComponent => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::RegExpPrototypeTest => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::StringPrototypeMatch
            | StandardBuiltinId::StringPrototypeMatchAll
            | StandardBuiltinId::StringPrototypeReplace
            | StandardBuiltinId::StringPrototypeReplaceAll
            | StandardBuiltinId::StringPrototypeSearch
            | StandardBuiltinId::StringPrototypeSplit
            | StandardBuiltinId::RegExpPrototypeCompile
            | StandardBuiltinId::RegExpPrototypeExec
            | StandardBuiltinId::RegExpPrototypeToString
            | StandardBuiltinId::RegExpPrototypeSymbolMatch
            | StandardBuiltinId::RegExpPrototypeSymbolReplace
            | StandardBuiltinId::RegExpPrototypeSymbolSearch
            | StandardBuiltinId::RegExpPrototypeSymbolSplit => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::StringPrototypeCodePointAt => (
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::Number)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::StringPrototypeAt => (
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::String)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::StringPrototypeIsWellFormed => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::BooleanConstructor => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                Self::boxed_primitive_instance_info(ValueInfo::new(ValueKind::Boolean)),
            ),
            StandardBuiltinId::SymbolConstructor => (
                ValueKind::Symbol,
                KindSet::from_kind(ValueKind::Symbol),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SymbolFor => (
                ValueKind::Symbol,
                KindSet::from_kind(ValueKind::Symbol),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SymbolKeyFor => (
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::String)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SymbolPrototypeDescriptionGetter => (
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::String)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SymbolPrototypeToString => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SymbolPrototypeValueOf
            | StandardBuiltinId::SymbolPrototypeToPrimitive => (
                ValueKind::Symbol,
                KindSet::from_kind(ValueKind::Symbol),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::BooleanPrototypeToString => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::BooleanPrototypeValueOf => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ErrorConstructor
            | StandardBuiltinId::EvalErrorConstructor
            | StandardBuiltinId::AggregateErrorConstructor
            | StandardBuiltinId::SuppressedErrorConstructor
            | StandardBuiltinId::RangeErrorConstructor
            | StandardBuiltinId::SyntaxErrorConstructor
            | StandardBuiltinId::TypeErrorConstructor
            | StandardBuiltinId::URIErrorConstructor
            | StandardBuiltinId::ReferenceErrorConstructor => (
                // A catalogue signature has no program point, so it cannot
                // name the error's current prototype chain; call analysis
                // uses `standard_error_instance_info` for that.
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::PromiseConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::promise_instance_shape()),
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::PromiseResolve
            | StandardBuiltinId::PromiseTry
            | StandardBuiltinId::PromiseReject
            | StandardBuiltinId::PromiseAll
            | StandardBuiltinId::PromiseAllSettled
            | StandardBuiltinId::PromiseAllKeyed
            | StandardBuiltinId::PromiseAllSettledKeyed
            | StandardBuiltinId::PromiseAny
            | StandardBuiltinId::PromiseRace => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::promise_instance_shape()),
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::PromiseWithResolvers => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::PromisePrototypeThen
            | StandardBuiltinId::PromisePrototypeCatch
            | StandardBuiltinId::PromisePrototypeFinally
            | StandardBuiltinId::PromiseThenFinally
            | StandardBuiltinId::PromiseCatchFinally
            | StandardBuiltinId::PromiseValueThunk
            | StandardBuiltinId::PromiseThrower
            | StandardBuiltinId::PromiseSpeciesGetter => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::PromiseCapabilityExecutor
            | StandardBuiltinId::PromiseAllResolveElement
            | StandardBuiltinId::PromiseAllSettledResolveElement
            | StandardBuiltinId::PromiseAllSettledRejectElement
            | StandardBuiltinId::PromiseAnyRejectElement
            | StandardBuiltinId::PromiseAllKeyedResolveElement
            | StandardBuiltinId::PromiseAllSettledKeyedResolveElement
            | StandardBuiltinId::PromiseAllSettledKeyedRejectElement
            | StandardBuiltinId::PromiseResolveFunction
            | StandardBuiltinId::PromiseRejectFunction
            | StandardBuiltinId::ArrayFromAsyncFulfilled
            | StandardBuiltinId::ArrayFromAsyncRejected => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::MapConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::map_instance_shape()),
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::MapSpeciesGetter => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::MapGroupBy => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::map_instance_shape()),
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::MapPrototypeClear | StandardBuiltinId::MapPrototypeForEach => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::MapPrototypeKeys
            | StandardBuiltinId::MapPrototypeValues
            | StandardBuiltinId::MapPrototypeEntries => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::map_iterator_instance_shape()),
                Self::value_info_from_shape(Some(Self::map_iterator_instance_shape())),
            ),
            StandardBuiltinId::MapIteratorNext => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Box::new(Self::empty_object_shape())),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::MapPrototypeDelete | StandardBuiltinId::MapPrototypeHas => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::MapPrototypeGet
            | StandardBuiltinId::MapPrototypeGetOrInsert
            | StandardBuiltinId::MapPrototypeGetOrInsertComputed => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::MapPrototypeSet => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::map_instance_shape()),
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::MapPrototypeSizeGetter => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::WeakMapConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::weak_map_instance_shape()),
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::WeakMapPrototypeDelete | StandardBuiltinId::WeakMapPrototypeHas => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::WeakMapPrototypeGet
            | StandardBuiltinId::WeakMapPrototypeGetOrInsert
            | StandardBuiltinId::WeakMapPrototypeGetOrInsertComputed => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::WeakMapPrototypeSet => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::weak_map_instance_shape()),
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::WeakSetConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::weak_set_instance_shape()),
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::WeakSetPrototypeAdd => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::weak_set_instance_shape()),
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::WeakSetPrototypeDelete | StandardBuiltinId::WeakSetPrototypeHas => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::WeakRefConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::weak_ref_instance_shape()),
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::WeakRefPrototypeDeref => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::FinalizationRegistryConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::finalization_registry_instance_shape()),
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::FinalizationRegistryPrototypeRegister => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::FinalizationRegistryPrototypeUnregister => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            // The stack return shapes are deliberately `None`: neither family
            // has a static instance shape, so the lowerer learns the kind and
            // nothing else and every member access stays dynamic.
            //
            // The fourth member is `fresh_constructed_instance_info()`, NOT the
            // `ValueInfo::undefined()` the lane note proposed. This builtin is
            // in `constructable()` (`builtins.rs`), so both consumers of
            // `constructor_instance` are reachable — `lower_class`'s
            // `inherited_instance` for `class D extends AsyncDisposableStack {}`
            // and `lower_new_expression`'s `null_heritage_return_path` else-arm
            // for a direct `new AsyncDisposableStack()`. Spelling it
            // `undefined` types the instance as nullish and makes
            // `emit_method_call`'s statically-nullish shortcut emit no call at
            // all; that is the measured batch-5 `IteratorConstructor` defect and
            // the batch-7 `IntlDateTimeFormatConstructor` defect verbatim, whose
            // arm 20 lines below is the precedent this copies (`Object`,
            // `{Object}`, no return shape, fresh constructed instance).
            StandardBuiltinId::TextEncoderConstructor
            | StandardBuiltinId::TextDecoderConstructor
            | StandardBuiltinId::TextEncoderPrototypeEncode
            | StandardBuiltinId::TextEncoderPrototypeEncodeInto
            | StandardBuiltinId::AsyncDisposableStackConstructor
            | StandardBuiltinId::DisposableStackConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                Self::fresh_constructed_instance_info(),
            ),
            // `use` and `adopt` both return their first argument unchanged.
            StandardBuiltinId::AsyncDisposableStackPrototypeUse
            | StandardBuiltinId::AsyncDisposableStackPrototypeAdopt
            | StandardBuiltinId::DisposableStackPrototypeUse
            | StandardBuiltinId::DisposableStackPrototypeAdopt => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::AsyncDisposableStackPrototypeDefer
            | StandardBuiltinId::DisposableStackPrototypeDefer
            | StandardBuiltinId::DisposableStackPrototypeDispose
            | StandardBuiltinId::AsyncDisposableStackDisposeAsyncFulfilled
            | StandardBuiltinId::AsyncDisposableStackDisposeAsyncRejected => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            // `move` returns a fresh stack; `disposeAsync` always returns a
            // promise, including on every failure path.
            StandardBuiltinId::AsyncDisposableStackPrototypeMove
            | StandardBuiltinId::DisposableStackPrototypeMove
            | StandardBuiltinId::AsyncDisposableStackPrototypeDisposeAsync => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TextDecoderPrototypeFatalGetter
            | StandardBuiltinId::TextDecoderPrototypeIgnoreBOMGetter
            | StandardBuiltinId::AsyncDisposableStackPrototypeDisposedGetter
            | StandardBuiltinId::DisposableStackPrototypeDisposedGetter => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::TextEncoderPrototypeEncodingGetter
            | StandardBuiltinId::TextDecoderPrototypeEncodingGetter
            | StandardBuiltinId::TextDecoderPrototypeDecode => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SetConstructor => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::set_instance_shape()),
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::SetSpeciesGetter => (
                ValueKind::Dynamic,
                KindSet::all_runtime_tags(),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SetPrototypeAdd => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::set_instance_shape()),
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::SetPrototypeDifference
            | StandardBuiltinId::SetPrototypeIntersection
            | StandardBuiltinId::SetPrototypeSymmetricDifference
            | StandardBuiltinId::SetPrototypeUnion => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::set_instance_shape()),
                Self::fresh_constructed_instance_info(),
            ),
            StandardBuiltinId::SetPrototypeIsDisjointFrom
            | StandardBuiltinId::SetPrototypeIsSubsetOf
            | StandardBuiltinId::SetPrototypeIsSupersetOf => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SetPrototypeClear | StandardBuiltinId::SetPrototypeForEach => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SetPrototypeValues | StandardBuiltinId::SetPrototypeEntries => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Self::set_iterator_instance_shape()),
                Self::value_info_from_shape(Some(Self::set_iterator_instance_shape())),
            ),
            StandardBuiltinId::SetIteratorNext => (
                ValueKind::Object,
                KindSet::from_kind(ValueKind::Object),
                Some(Box::new(Self::empty_object_shape())),
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SetPrototypeDelete | StandardBuiltinId::SetPrototypeHas => (
                ValueKind::Boolean,
                KindSet::from_kind(ValueKind::Boolean),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::SetPrototypeSizeGetter => (
                ValueKind::Number,
                KindSet::from_kind(ValueKind::Number),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ErrorPrototypeStackGetter => (
                ValueKind::Dynamic,
                KindSet::from_kind(ValueKind::String)
                    .union(KindSet::from_kind(ValueKind::Undefined)),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ErrorPrototypeStackSetter => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
            StandardBuiltinId::ErrorPrototypeToString => (
                ValueKind::String,
                KindSet::from_kind(ValueKind::String),
                None,
                ValueInfo::undefined(),
            ),
            // The abstract constructor always throws. Under the current
            // SourceText-only host no non-JS module-source object can reach the
            // native tag getter, whose result is therefore Undefined.
            StandardBuiltinId::ThrowTypeError
            | StandardBuiltinId::AbstractModuleSourceConstructor
            | StandardBuiltinId::AbstractModuleSourcePrototypeToStringTagGetter => (
                ValueKind::Undefined,
                KindSet::from_kind(ValueKind::Undefined),
                None,
                ValueInfo::undefined(),
            ),
        };

        FunctionSignature {
            id: builtin.function_id(),
            to_string_representation: builtin
                .native_function_name()
                .map(|name| CallableToStringRepresentation::NativeNamed(name.to_string()))
                .unwrap_or(CallableToStringRepresentation::NativeAnonymous),
            protocol: if builtin.constructable() {
                FunctionProtocolIr::OrdinaryCallAndConstruct
            } else {
                FunctionProtocolIr::OrdinaryCallOnly
            },
            callable: true,
            class_heritage_kind: ClassHeritageKind::None,
            params: Vec::new(),
            return_kind,
            return_possible_kinds,
            return_shape: FunctionReturnShape::flow_sensitive(return_shape),
            return_targets: FunctionTargetKnowledge::unknown(),
            constructor_instance,
            // Intrinsics have no lexical-this captures. Until the first call
            // observes a receiver, retaining the complete global shape here
            // would duplicate it in every cloned builtin signature.
            this_info: ValueInfo::undefined(),
            this_observed: false,
            source_call_flow_effects: SourceCallFlowEffects::unobserved(),
        }
    }
}

#[cfg(test)]
mod uint8_array_codec_tests {
    use super::*;

    #[test]
    fn uint8_array_codecs_are_owned_only_by_the_uint8_subtype() {
        for constructor in [
            StandardBuiltinId::Uint8ArrayConstructor,
            StandardBuiltinId::Uint8ClampedArrayConstructor,
            StandardBuiltinId::Int8ArrayConstructor,
            StandardBuiltinId::TypedArrayConstructor,
        ] {
            let constructor_shape = ScriptLowerer::standard_builtin_function_shape(constructor);
            let prototype_shape =
                ScriptLowerer::typed_array_constructor_prototype_shape(constructor);
            let expected = constructor == StandardBuiltinId::Uint8ArrayConstructor;
            for (name, _) in UINT8_ARRAY_CODEC_STATIC_MEMBERS {
                assert_eq!(
                    read_heap_shape_property(&constructor_shape, name).is_some(),
                    expected
                );
            }
            for (name, _) in UINT8_ARRAY_CODEC_PROTOTYPE_MEMBERS {
                assert_eq!(
                    read_heap_shape_property(&prototype_shape, name).is_some(),
                    expected
                );
                assert!(read_heap_shape_property(
                    &ScriptLowerer::typed_array_prototype_shape(),
                    name
                )
                .is_none());
            }
        }
    }

    #[test]
    fn uint8_array_codecs_keep_static_factory_shapes_and_live_instance_calls() {
        for source in ["Uint8Array.fromBase64('');", "Uint8Array.fromHex('');"] {
            let result = codec_call_result(source);
            assert_eq!(result.kind, ValueKind::Object);
            let shape = result.heap_shape.expect("Uint8Array result shape");
            for (name, builtin) in UINT8_ARRAY_CODEC_PROTOTYPE_MEMBERS {
                let Some(ObjectShapeProperty::Data(method)) =
                    read_heap_shape_property(&shape, name)
                else {
                    panic!("returned Uint8Array must expose {name}");
                };
                assert_eq!(
                    method.function_targets.exact_single_target(),
                    Some(&builtin.function_id())
                );
            }
        }
        for (method, builtin) in UINT8_ARRAY_CODEC_PROTOTYPE_MEMBERS {
            let argument = if method.starts_with("set") { "''" } else { "" };
            let result = codec_call_result(&format!("new Uint8Array(0).{method}({argument});"));
            let ExprIr::MaterializeBinding { name, value, body } = &result.expr else {
                panic!("the constructed receiver is acquired once: {result:?}");
            };
            assert!(matches!(value.expr, ExprIr::Construct { .. }));
            let ExprIr::CallIndirect {
                callee,
                this_arg: Some(receiver),
                args,
                ..
            } = &body.expr
            else {
                panic!("the actual method is called: {body:?}");
            };
            assert!(matches!(&receiver.expr, ExprIr::Identifier(storage) if storage == name));
            assert!(callee
                .function_targets
                .known_targets()
                .contains(&builtin.function_id()));
            assert!(callee.function_targets.exact_targets().is_none());
            assert!(match &callee.expr {
                ExprIr::PropertyRead {
                    target,
                    key: PropertyKeyIr::StaticString(key),
                } =>
                    matches!(&target.expr, ExprIr::Identifier(storage) if storage == name)
                        && key == method,
                ExprIr::SpecOperation {
                    operation: SpecOperationIr::GetV,
                    operands,
                } =>
                    operands.len() == 2
                        && matches!(&operands[0].expr, ExprIr::Identifier(storage) if storage == name)
                        && matches!(&operands[1].expr, ExprIr::String(key) if key == method),
                _ => false,
            });
            assert_eq!(args.len(), usize::from(!argument.is_empty()));
            if let Some(argument) = args.first() {
                assert!(matches!(&argument.expr, ExprIr::String(value) if value.is_empty()));
            }
            // Constructor effects prevent a frozen prototype snapshot from
            // licensing the native method's result. Native codec regressions
            // cover the actual read/written records and returned strings.
            assert_eq!(result.kind, ValueKind::Dynamic);
            assert_eq!(result.possible_kinds, KindSet::all_runtime_tags());
            assert!(result.heap_shape.is_none());
        }
    }

    fn codec_call_result(source: &str) -> TypedExpr {
        let program = crate::lower(
            &scriptw_front::parse(source, scriptw_front::ParseOptions::script()).unwrap(),
        );
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.unwrap();
        let Some(StatementIr::Expression(result)) = script.body.statements.last() else {
            panic!("codec call must remain a value-producing expression");
        };
        result.clone()
    }
}
