// puy-test: test-ext
// The declarations of an extension's unit that break the rules of `native`
// (`docs/spec/extensions.md` 2): a body or `forward` (E0771), the shape of a class with `native`
// methods (E0772), and the parameters, results and overloads a Rust function cannot take
// (E0773). They are reported in the unit's source.
program ExtUnitErrors;

uses TestExtBad;

begin
end.
