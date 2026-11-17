program DefaultBuiltinTypes;

// `Default(T)` takes the built-in types of `System` as type names (types.md chapter 6): the
// record `TIndexRange` (objects.md 12.3), the class `TObject` and the anonymous method type
// `TProc`.

var
  R: TIndexRange;
  O: TObject;
  P: TProc;
begin
  R := Default(TIndexRange);
  O := Default(TObject);
  P := Default(TProc);
  WriteLn(R.First, ' ', R.Last, ' ', O = nil, ' ', Assigned(P));
end.
