program ValueEqualityErrors;

// E0767 and the `[ValueEquality]` variants of E0765 (equality.md 3.4, 3.5): `[ValueEquality]` on
// a record (notes) or another type, `[EqualityMember]` outside a class treated as
// `[ValueEquality]` (a record field, a `class var`, a class without it, a descendant that stopped
// it by overriding both), on a `[weak]` field, and `[ValueEquality]` written with the user's
// `Equals` / `GetHashCode` or with operator `Equal`. The misplaced attributes are removed by safe
// fixes. A declaration with an E0767 gets no E0765 (3.5).

type
  [ValueEquality]
  TPoint = record
    X, Y: Integer;
  end;

  [ValueEquality]
  TColor = (Red, Green);

  TPair = record
    [EqualityMember] A: Integer;
    B: Integer;
  end;

  TPlain = class
  public
    [EqualityMember] V: Integer;
  end;

  [ValueEquality]
  TItem = class(TRefObject)
  public
    [EqualityMember, weak] Parent: TItem;
    Name: string;
  public
    class var [EqualityMember] Count: Integer;
  end;

  [ValueEquality]
  TBase = class
  public
    V: Integer;
  end;

  // Stops `[ValueEquality]` by overriding both: its members are misplaced.
  TCustom = class(TBase)
  public
    [EqualityMember] W: Integer;
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
  end;

  [ValueEquality]
  TOverriding = class
  public
    V: Integer;
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
  end;

  [ValueEquality]
  TWithEqual = class(TRefObject)
  public
    V: Integer;
    class operator Equal(const A, B: TWithEqual): Boolean;
    function GetHashCode: Int64; override;
  end;

  // An E0767 hides the missing `GetHashCode` (E0765) of the record.
  [ValueEquality]
  TKey = record
    V: Integer;
    class operator Equal(const A, B: TKey): Boolean;
  end;

function TCustom.Equals(Obj: TObject): Boolean;
begin
  Result := False;
end;

function TCustom.GetHashCode: Int64;
begin
  Result := 0;
end;

function TOverriding.Equals(Obj: TObject): Boolean;
begin
  Result := False;
end;

function TOverriding.GetHashCode: Int64;
begin
  Result := 0;
end;

class operator TWithEqual.Equal(const A, B: TWithEqual): Boolean;
begin
  Result := A.V = B.V;
end;

function TWithEqual.GetHashCode: Int64;
begin
  Result := V;
end;

class operator TKey.Equal(const A, B: TKey): Boolean;
begin
  Result := A.V = B.V;
end;

begin
end.
