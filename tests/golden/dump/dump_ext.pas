// puy-test: test-ext
// The calls of `native` routines in the dumps (`docs/spec/extensions.md` 4, 6): `CallExt` with
// the index of the extension function section, the sections `.extunit` and `.ext`, and the
// signature of a callable argument, and the shapes of records and dynamic arrays.
program DumpExt;

uses TestExt;

var
  C: TCounter;
  S: TSpan;
begin
  C := TCounter.Create(Add(1, 2));
  C.Increment;
  WriteLn(C.Value, ' ', Upper('a'), ' ', TCounter.Live);
  WriteLn(Apply(function(X: Integer): Integer
    begin
      Result := X;
    end, 1));
  S := ShiftSpan(S, 2);
  WriteLn(S.Start, ' ', Length(MakeEntries(['x'])));
end.
