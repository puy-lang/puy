program DumpEqualsHash;

// The dumps of `TObject.Equals` / `GetHashCode` (equality.md 8.2, 9): `inherited Equals(Obj)` is
// `EqInt` and `inherited GetHashCode` is `HashInt` of `Self` (the built-in bodies k = 3 and 4
// expanded), calls of the two are `CallVirtual` 4 and 5, and the table of a class overriding them
// holds its functions at 4 and 5 (`builtin3` / `builtin4` otherwise).

type
  TKey = class
  public
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
  end;

  TPlain = class
  end;

function TKey.Equals(Obj: TObject): Boolean;
begin
  Result := inherited Equals(Obj);
end;

function TKey.GetHashCode: Int64;
begin
  Result := inherited GetHashCode;
end;

var
  K: TKey;
  P: TPlain;
begin
  K := TKey.Create;
  P := TPlain.Create;
  WriteLn(K.Equals(P), ' ', K.GetHashCode = K.GetHashCode);
  K.Free;
  P.Free;
end.
