program Main;

// The generated `Equals` (equality.md 3.3, 8.3) of a unit's class and of the program's
// descendant come after every unit's functions, in the order generated, and the unit's checks
// that the instance is not of the descendant declaring `Equal` again in the program. The
// instantiation's body comes after them.

uses Keys;

type
  TSubKey = class(TKey)
    class operator Equal(const A, B: TSubKey): Boolean;
    function GetHashCode: Int64; override;
  end;

  TBox<T> = class(TRefObject)
    class operator Equal(const A, B: TBox<T>): Boolean;
    function GetHashCode: Int64; override;
  end;

class operator TSubKey.Equal(const A, B: TSubKey): Boolean;
begin
  Result := A.V = B.V;
end;

function TSubKey.GetHashCode: Int64;
begin
  Result := V;
end;

class operator TBox<T>.Equal(const A, B: TBox<T>): Boolean;
begin
  Result := True;
end;

function TBox<T>.GetHashCode: Int64;
begin
  Result := 0;
end;

var
  K: TKey;
  S: TSubKey;
  B: TBox<Integer>;
begin
  K := TKey.Create(1);
  S := TSubKey.Create(1);
  B := TBox<Integer>.Create;
  WriteLn(K.Equals(TKey.Create(1)), ' ', K.Equals(S), ' ', S.Equals(K), ' ', B.Equals(B));
end.
