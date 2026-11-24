program RttiTypeInfoErrors;

// TypeInfo takes a type, not an expression (E0776; rtti.md 2.2) nor a helper (E0753; 1.4). A
// PTypeInfo is opaque: no ordinal, order, arithmetic, conversion, Write or ^ (2.1)

type
  TIntHelper = record helper for Integer
    function Twice: Integer;
  end;

function TIntHelper.Twice: Integer;
begin
  Result := Self * 2;
end;

var
  P: PTypeInfo;
  I, X: Integer;
begin
  X := 1;
  P := TypeInfo(X);
  P := TypeInfo(1 + 2);
  P := TypeInfo(Integer);
  P := TypeInfo(TIntHelper);
  I := Ord(P);
  WriteLn(P < P);
  WriteLn(P);
  I := Integer(P);
  P := P + 1;
  WriteLn(P^);
  WriteLn(I);
end.
