program EscapedReservedNames;

// Only a member may take a reserved name, and only written with `&` (semantics.md 4.1):
// a member without `&` and a local variable with it are E0701. An identifier with `&` and
// the same one without it are the same name, so declaring both is E0701 (lexical.md 3.2).

type
  TRec = record
    function Result: Integer;
    Self: Integer;
  end;

function TRec.Result: Integer;
begin
  Result := 1;
end;

var
  &Result: Integer;
  Foo, &Foo: Integer;
begin
end.
