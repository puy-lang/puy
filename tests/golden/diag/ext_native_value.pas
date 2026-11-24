// puy-test: test-ext
// What a `native` routine or an extension class cannot be used as (E0774;
// `docs/spec/extensions.md` 2.2, 2.3): a routine as a value, a constructor through a class
// reference, and the creation of an extension class without a `native` constructor.
program ExtNativeValue;

uses TestExt;

type
  TCounterClass = class of TCounter;
  TGetter = reference to function: Integer;

var
  F: TIntFunc;
  R: TCounterClass;
  C: TCounter;
  T: TTicket;
  G: TGetter;
begin
  F := Inc1;
  R := TCounter;
  C := R.Create(1);
  T := TTicket.Create;
  C := TCounter.Create(1);
  G := C.Value;
end.
