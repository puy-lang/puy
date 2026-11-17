program ValueEqualityPositions;

// The positions and combinations of the equality declarations left out by the other cases
// (equality.md 3.2, 3.4, 3.5, 4; attributes.md 3): E0767 for `[ValueEquality]` on an interface,
// a field or a method and `[EqualityMember]` on a method or on a descendant that moved to the
// generated `Equals` of 3.3, with the fix removing only that attribute from a list; E0705 for an
// argument; with `[ValueEquality]`, only the combination's E0765 (for `Equal` with overrides, a
// single override, `Equal` without `GetHashCode`); the one-sided E0765 for a class that only
// inherits `[ValueEquality]`; E0765 for a record `GetHashCode` with parameters while a private one
// is accepted; and E0713 / E0752 / E0761 for `KeyEquals` and `HashCombine`.

type
  TagAttribute = class(TCustomAttribute)
  end;

  [ValueEquality]
  IFoo = interface
  end;

  TA = class
  public
    [ValueEquality] F: Integer;
    [ValueEquality] procedure M;
  end;

  [ValueEquality]
  TB = class
  public
    V: Integer;
    [EqualityMember] procedure M;
  end;

  [ValueEquality, Tag]
  TList2 = record
    V: Integer;
  end;

  [ValueEquality]
  TBoth = class(TRefObject)
    V: Integer;
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
    class operator Equal(const A, B: TBoth): Boolean;
  end;

  [ValueEquality]
  TOnly = class
    V: Integer;
    function GetHashCode: Int64; override;
  end;

  [ValueEquality]
  TRc = class(TRefObject)
    V: Integer;
    class operator Equal(const A, B: TRc): Boolean;
  end;

  [ValueEquality]
  TBase = class(TRefObject)
    V: Integer;
  end;

  TOneSide = class(TBase)
    function Equals(Obj: TObject): Boolean; override;
  end;

  TValEq = class(TBase)
    [Tag, EqualityMember] W: Integer;
    class operator Equal(const A, B: TValEq): Boolean;
    function GetHashCode: Int64; override;
  end;

  [ValueEquality(1)]
  TArg = class
    V: Integer;
  end;

  TR = record
    Id: Integer;
    class operator Equal(const A, B: TR): Boolean;
    function GetHashCode(X: Integer): Int64;
  end;

  TPriv = record
  private
    function GetHashCode: Int64;
  public
    Id: Integer;
    class operator Equal(const A, B: TPriv): Boolean;
  end;

  TF = function(H, X: Int64): Int64;

const
  C1 = HashCombine(1, 2);
  C2 = KeyEquals(1, 1);

var
  F: TF;

procedure TA.M;
begin
end;

procedure TB.M;
begin
end;

function TBoth.Equals(Obj: TObject): Boolean;
begin
  Result := False;
end;

function TBoth.GetHashCode: Int64;
begin
  Result := 0;
end;

class operator TBoth.Equal(const A, B: TBoth): Boolean;
begin
  Result := True;
end;

function TOnly.GetHashCode: Int64;
begin
  Result := 0;
end;

class operator TRc.Equal(const A, B: TRc): Boolean;
begin
  Result := True;
end;

function TOneSide.Equals(Obj: TObject): Boolean;
begin
  Result := False;
end;

class operator TValEq.Equal(const A, B: TValEq): Boolean;
begin
  Result := True;
end;

function TValEq.GetHashCode: Int64;
begin
  Result := 0;
end;

class operator TR.Equal(const A, B: TR): Boolean;
begin
  Result := True;
end;

function TR.GetHashCode(X: Integer): Int64;
begin
  Result := 0;
end;

class operator TPriv.Equal(const A, B: TPriv): Boolean;
begin
  Result := True;
end;

function TPriv.GetHashCode: Int64;
begin
  Result := 0;
end;

begin
  F := HashCombine;
  WriteLn(HashCombine<Integer>(1, 2));
end.
