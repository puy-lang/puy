program RefClassHashErrors;

// E0765 for reference counted classes (equality.md 3.3, 3.5, 3.6): a class declaring its own
// `Equal` must override `GetHashCode` (notes and a suggestion inserting the declaration), also
// when a descendant declares `Equal` again, and cannot override `Equals`, which is generated (only
// that error, before the missing `GetHashCode`). `Equal` with operands of other types needs
// nothing, and a `GetHashCode` override that failed (E0744) is not reported again.

type
  TOnlyEqual = class(TRefObject)
    V: Integer;
    class operator Equal(const A, B: TOnlyEqual): Boolean;
  end;

  TWithEquals = class(TRefObject)
    V: Integer;
    class operator Equal(const A, B: TWithEquals): Boolean;
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
  end;

  // Both errors would apply: only the combination is reported (equality.md 3.5).
  TEqualsNoHash = class(TRefObject)
    class operator Equal(const A, B: TEqualsNoHash): Boolean;
    function Equals(Obj: TObject): Boolean; override;
  end;

  TKey = class(TRefObject)
    V: Integer;
    class operator Equal(const A, B: TKey): Boolean;
    function GetHashCode: Int64; override;
  end;

  // Declaring `Equal` again in a descendant asks for `GetHashCode` again.
  TSubKey = class(TKey)
    class operator Equal(const A, B: TSubKey): Boolean;
  end;

  // A descendant that does not declare `Equal` needs nothing.
  TNamedKey = class(TKey)
    Name: string;
  end;

  // `Equal` with an `Integer` operand is not the class's own `Equal`.
  TMixed = class(TRefObject)
    V: Integer;
    class operator Equal(const A: TMixed; B: Integer): Boolean;
  end;

  // The `Integer` result is E0744 only.
  TIntegerHash = class(TRefObject)
    class operator Equal(const A, B: TIntegerHash): Boolean;
    function GetHashCode: Int64; override;
  end;

class operator TOnlyEqual.Equal(const A, B: TOnlyEqual): Boolean;
begin
  Result := A.V = B.V;
end;

class operator TWithEquals.Equal(const A, B: TWithEquals): Boolean;
begin
  Result := A.V = B.V;
end;

function TWithEquals.Equals(Obj: TObject): Boolean;
begin
  Result := False;
end;

function TWithEquals.GetHashCode: Int64;
begin
  Result := V;
end;

class operator TEqualsNoHash.Equal(const A, B: TEqualsNoHash): Boolean;
begin
  Result := True;
end;

function TEqualsNoHash.Equals(Obj: TObject): Boolean;
begin
  Result := True;
end;

class operator TKey.Equal(const A, B: TKey): Boolean;
begin
  Result := A.V = B.V;
end;

function TKey.GetHashCode: Int64;
begin
  Result := V;
end;

class operator TSubKey.Equal(const A, B: TSubKey): Boolean;
begin
  Result := A.V = B.V;
end;

class operator TMixed.Equal(const A: TMixed; B: Integer): Boolean;
begin
  Result := A.V = B;
end;

class operator TIntegerHash.Equal(const A, B: TIntegerHash): Boolean;
begin
  Result := True;
end;

function TIntegerHash.GetHashCode: Integer;
begin
  Result := 0;
end;

begin
end.
