//! Storm compiler's internal function table.
use crate::Type;

// Names, parameter counts and result types follow IntFuncTable in
// src/libs/core/src/internal_functions.cpp. UNKNOWN has a dynamic result.
const SIGNATURES: &[(&str, usize, Option<Type>)] = &[
    ("abs", 1, None),
    ("acos", 1, Some(Type::Float)),
    ("and", 2, Some(Type::Int)),
    ("argb", 4, Some(Type::Int)),
    ("asin", 1, Some(Type::Float)),
    ("atan", 1, Some(Type::Float)),
    ("atan2", 2, Some(Type::Float)),
    ("breakpoint", 0, Some(Type::Void)),
    ("checkattribute", 2, Some(Type::Int)),
    ("checkfunction", 1, Some(Type::Int)),
    ("clearentityap", 1, Some(Type::Void)),
    ("clearevents", 0, Some(Type::Void)),
    ("clearpostevents", 0, Some(Type::Void)),
    ("clearref", 1, Some(Type::Void)),
    ("copyattributes", 2, Some(Type::Void)),
    ("cos", 1, Some(Type::Float)),
    ("createclass", 1, Some(Type::Object)),
    ("createcontrol", 1, Some(Type::Int)),
    ("createentity", 2, Some(Type::Int)),
    ("deleteattribute", 2, Some(Type::Void)),
    ("deleteclass", 1, Some(Type::Void)),
    ("deletecontrol", 1, Some(Type::Void)),
    ("deleteentities", 0, Some(Type::Void)),
    ("deleteentitiesbytype", 1, Some(Type::Void)),
    ("deleventhandler", 2, Some(Type::Void)),
    ("dumpattributes", 1, Some(Type::Void)),
    ("entityupdate", 1, Some(Type::Void)),
    ("event", 0, Some(Type::Void)),
    ("eventsbreak", 0, Some(Type::Void)),
    ("exitprogram", 0, Some(Type::Void)),
    ("findentity", 2, Some(Type::Int)),
    ("findentitynext", 1, Some(Type::Int)),
    ("findsubstr", 3, Some(Type::String)),
    ("frnd", 0, Some(Type::Float)),
    ("fts", 2, Some(Type::String)),
    ("getarraysize", 1, Some(Type::Int)),
    ("getattributen", 2, Some(Type::AReference)),
    ("getattributename", 1, Some(Type::String)),
    ("getattributesnum", 1, Some(Type::Int)),
    ("getattributevalue", 1, Some(Type::String)),
    ("getdeltatime", 0, Some(Type::Int)),
    ("getengineversion", 0, Some(Type::Int)),
    ("getentity", 2, Some(Type::Int)),
    ("geteventdata", 0, None),
    ("getsymbol", 2, Some(Type::String)),
    ("gettargetplatform", 0, Some(Type::String)),
    ("isdigit", 2, Some(Type::Int)),
    ("isentity", 1, Some(Type::Int)),
    ("layeraddobject", 3, Some(Type::Void)),
    ("layerdeletecontent", 1, Some(Type::Void)),
    ("layerdelobject", 2, Some(Type::Void)),
    ("layerfreeze", 2, Some(Type::Void)),
    ("layersetexecute", 1, Some(Type::Void)),
    ("layersetmessages", 2, Some(Type::Void)),
    ("layersetrealize", 1, Some(Type::Void)),
    ("loadenginestate", 1, Some(Type::Void)),
    ("loadsegment", 1, Some(Type::Int)),
    ("loadvariable", 2, Some(Type::Int)),
    ("lockcontrol", 2, Some(Type::Void)),
    ("makefloat", 1, Some(Type::Float)),
    ("makeint", 1, Some(Type::Int)),
    ("mapcontrol", 2, Some(Type::Void)),
    ("or", 2, Some(Type::Int)),
    ("postevent", 0, Some(Type::Void)),
    ("pow", 2, Some(Type::Float)),
    ("rand", 1, Some(Type::Int)),
    ("saveenginestate", 1, Some(Type::Void)),
    ("savevariable", 2, Some(Type::Int)),
    ("segmentisloaded", 1, Some(Type::Int)),
    ("sendmessage", 0, Some(Type::Ptr)),
    ("setarraysize", 2, Some(Type::Void)),
    ("setcontrolflags", 2, Some(Type::Void)),
    ("setcontroltreshold", 2, Some(Type::Void)),
    ("seteventhandler", 3, Some(Type::Void)),
    ("settimescale", 1, Some(Type::Void)),
    ("shl", 2, Some(Type::Int)),
    ("shr", 2, Some(Type::Int)),
    ("sin", 1, Some(Type::Float)),
    ("sort", 1, Some(Type::Void)),
    ("sqr", 1, Some(Type::Float)),
    ("sqrt", 1, Some(Type::Float)),
    ("stf", 1, Some(Type::Float)),
    ("sti", 1, Some(Type::Int)),
    ("stop", 0, Some(Type::Void)),
    ("strcut", 3, Some(Type::String)),
    ("strlen", 1, Some(Type::Int)),
    ("tan", 1, Some(Type::Float)),
    ("testref", 1, Some(Type::Int)),
    ("trace", 1, Some(Type::Void)),
    ("unloadsegment", 1, Some(Type::Void)),
    ("vartype", 1, Some(Type::String)),
];

pub(crate) fn signature(name: &str) -> Option<(usize, Option<Type>)> {
    let index = SIGNATURES
        .binary_search_by_key(&name, |(entry, _, _)| *entry)
        .ok()?;
    let (_, arity, result) = SIGNATURES[index];
    Some((arity, result))
}

#[test]
fn retained_signatures_match_the_original_internal_function_table() {
    let cpp = include_str!("../../../../src/libs/core/src/internal_functions.cpp");
    let table = cpp
        .split_once("INTFUNCDESC IntFuncTable[] = {")
        .unwrap()
        .1
        .split_once("};")
        .unwrap()
        .0;
    let uncommented = table
        .lines()
        .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
        .collect::<Vec<_>>()
        .join(" ");
    let tokens: Vec<_> = uncommented.split(',').map(str::trim).collect();
    let mut rows = tokens.chunks_exact(3);
    assert!(rows.remainder().is_empty());
    let mut expected: Vec<_> = rows
        .by_ref()
        .map(|row| {
            let arity = row[0].parse().unwrap();
            let name = row[1].trim_matches('"').to_ascii_lowercase();
            let result = match row[2] {
                "TVOID" => Some(Type::Void),
                "VAR_INTEGER" => Some(Type::Int),
                "VAR_FLOAT" => Some(Type::Float),
                "VAR_PTR" => Some(Type::Ptr),
                "VAR_STRING" => Some(Type::String),
                "VAR_OBJECT" => Some(Type::Object),
                "VAR_AREFERENCE" => Some(Type::AReference),
                "UNKNOWN" => None,
                kind => panic!("unrecognized original VM type {kind}"),
            };
            (name, arity, result)
        })
        .collect();
    expected.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        expected
            .iter()
            .map(|(n, a, t)| (n.as_str(), *a, *t))
            .collect::<Vec<_>>(),
        SIGNATURES
    );
}
