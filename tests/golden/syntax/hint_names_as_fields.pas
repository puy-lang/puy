program HintNamesAsFields;

// After `class`, `platform` and `deprecated` followed by `:` are the names of fields, not hint
// directives (attributes.md 6.1): the class body is read as usual and the fields can be used.

type
  TFoo = class
    platform: Integer;
    deprecated: Integer;
  end;

var
  F: TFoo;
begin
  F := TFoo.Create;
  F.platform := 1;
  F.deprecated := 2;
  WriteLn(F.platform + F.deprecated);
  F.Free;
end.
