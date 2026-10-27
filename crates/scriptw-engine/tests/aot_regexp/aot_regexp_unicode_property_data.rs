use super::aot_regexp_property_escape::assert_normal;

#[test]
fn cold_and_repeated_property_lookups_survive_gc_in_independent_runtimes() {
    // Every assert_normal invocation creates separate strict/sloppy Stores and
    // actual R instances. A compiled Module cache must not share readiness.
    for _ in 0..2 {
        assert_normal(
            r#"
var cases=[
 ['ASCII','A','\u03a9'],
 ['General_Category=Uppercase_Letter','A','a'],
 ['gc=Lu','\u03a9','\u03c9'],
 ['Script=Greek','\u03a9','A'],
 ['sc=Grek','\u03c9','a'],
 ['Script_Extensions=Hiragana','\u30fc','A']
];
for(var round=0;round<3;round++){
 for(var row of cases){
  var yes=property(row[0],112,[117]),no=property(row[0],80,[117]);
  gc();
  require(yes.test(row[1]) && !yes.test(row[2]),'computed property alias membership after data publication');
  require(!no.test(row[1]) && no.test(row[2]),'property complement uses the same complete catalogue');
 }
}
var emoji=property('RGI_Emoji',112,[118]);gc();
require(emoji.test(codePoint(0x1f600)) && !emoji.test('x'),'string properties share the initialized image');
var input=computed([111,108,100],[103]);input.lastIndex=7;
input.compile(fromUnits([94].concat(propertyUnits('ASCII',112),[36])),fromUnits([117]));gc();
require(input.lastIndex===0 && input.test('A') && !input.test('\u03a9'),'recompile publishes a matcher after first and repeated catalogue use');
rejects([94].concat(propertyUnits('Script=greek',112),[36]),[117]);
rejects([94].concat(propertyUnits('RGI_Emoji',112),[36]),[117]);
rejects([94].concat(propertyUnits('RGI_Emoji',80),[36]),[118]);
print('regexp-property-data:ok');262;
"#,
            "regexp-property-data:ok",
        );
    }
}

#[test]
fn property_and_normalization_images_initialize_independently_in_either_order() {
    for normalization_first in [true, false] {
        let before = if normalization_first {
            "require('\u{00e9}'.normalize('NFD')==='e\u{0301}','normalization first');"
        } else {
            ""
        };
        let source = format!(
            r#"
{before}
var upper=property('Uppercase_Letter',112,[117]);gc();
require(upper.test('A') && !upper.test('a'),'catalogue initializes after optional normalization');
require('\u00e9'.normalize('NFD')==='e\u0301','normalization remains intact after catalogue initialization');
var greek=property('Script=Greek',112,[117]);gc();
require(greek.test('\u03a9') && !greek.test('A'),'catalogue remains intact after normalization');
require('\u212b'.normalize('NFD')==='A\u030a','normalization repeated use retains its own range');
require(upper.test('Z') && !upper.test('z'),'previous matcher survives both copies and GC');
print('regexp-property-data-order:ok');262;
"#
        );
        assert_normal(&source, "regexp-property-data-order:ok");
    }
}
