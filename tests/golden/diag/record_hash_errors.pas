program RecordHashErrors;

// E0765 for records (equality.md 3.2, 3.5, 3.6): a record declaring its own `Equal` must also
// declare `function GetHashCode: Int64;` (notes and a suggestion inserting the declaration), a
// record cannot declare only `GetHashCode`, and `GetHashCode` must have that exact shape.
// `Equal` with operands of other types, and `Equal` without `NotEqual`, need nothing more. A
// `GetHashCode` whose result type does not resolve gets only the type error.

type
  TOnlyEqual = record
    X, Y: Integer;
    class operator Equal(const A, B: TOnlyEqual): Boolean;
  end;

  TOnlyHash = record
    Id: Integer;
    function GetHashCode: Int64;
  end;

  TIntegerHash = record
    Id: Integer;
    class operator Equal(A, B: TIntegerHash): Boolean;
    function GetHashCode: Integer;
  end;

  TStaticHash = record
    Id: Integer;
    class operator Equal(const A, B: TStaticHash): Boolean;
    class function GetHashCode: Int64; static;
  end;

  // A result type that does not resolve is E0700 only, without E0765 on top of it.
  TUnknownHash = record
    Id: Integer;
    class operator Equal(const A, B: TUnknownHash): Boolean;
    function GetHashCode: TNoSuchType;
  end;

  // `Equal` with a `Double` operand is not the record's own `Equal`: no `GetHashCode` needed.
  TVec = record
    X: Double;
    class operator Equal(const A: TVec; B: Double): Boolean;
  end;

  // `Equal` and `GetHashCode` without `NotEqual`: fine.
  TKey = record
    Id: Integer;
    class operator Equal(const A, B: TKey): Boolean;
    function GetHashCode: Int64;
  end;

class operator TOnlyEqual.Equal(const A, B: TOnlyEqual): Boolean;
begin
  Result := (A.X = B.X) and (A.Y = B.Y);
end;

function TOnlyHash.GetHashCode: Int64;
begin
  Result := Id;
end;

class operator TIntegerHash.Equal(A, B: TIntegerHash): Boolean;
begin
  Result := A.Id = B.Id;
end;

function TIntegerHash.GetHashCode: Integer;
begin
  Result := Id;
end;

class operator TStaticHash.Equal(const A, B: TStaticHash): Boolean;
begin
  Result := A.Id = B.Id;
end;

class function TStaticHash.GetHashCode: Int64;
begin
  Result := 0;
end;

class operator TUnknownHash.Equal(const A, B: TUnknownHash): Boolean;
begin
  Result := A.Id = B.Id;
end;

class operator TVec.Equal(const A: TVec; B: Double): Boolean;
begin
  Result := A.X = B;
end;

class operator TKey.Equal(const A, B: TKey): Boolean;
begin
  Result := A.Id = B.Id;
end;

function TKey.GetHashCode: Int64;
begin
  Result := Id;
end;

begin
end.
