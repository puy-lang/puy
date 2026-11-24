program AssertValuesErrors;

// `Values` is a member of EAssertionFailed and its descendants only, and is read-only
// (docs/spec/exceptions.md 2.3).
var
  V: TArray<TAssertionValue>;
begin
  try
    Assert(False);
  except
    on E: EAssertionFailed do
    begin
      V := E.Values;
      E.Values := V;
    end;
    on E: Exception do
      V := E.Values;
  end;
  WriteLn(Length(V));
end.
