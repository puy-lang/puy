program EqualsHashErrors;

// The declarations of `Equals` and `GetHashCode` (equality.md 3.1, 3.6, chapter 10; objects.md
// 2.4): E0765 for a class overriding only one of them (with the declaration of the other as a
// suggestion), also when an ancestor overrides both and when an override of another overload of
// `Equals` comes first; W0021 for the two declared without `override`; E0744 for
// `GetHashCode: Integer` with the note and the fix to `Int64`, and without them for a return type
// that is not an integer.

type
  TOnlyEquals = class
  public
    function Equals(Obj: TObject): Boolean; override;
  end;

  TOnlyHash = class(TRefObject)
    function GetHashCode: Int64; override;
  end;

  TBoth = class
  public
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
  end;

  TChildEquals = class(TBoth)
  public
    function Equals(Obj: TObject): Boolean; override;
  end;

  TNoOverride = class
  public
    function Equals(Obj: TObject): Boolean;
    function GetHashCode: Int64;
  end;

  TOtherDialect = class
  public
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Integer; override;
  end;

  TOverloadBase = class
  public
    function Equals(Other: TOverloadBase): Boolean; reintroduce; overload; virtual;
  end;

  TOverloads = class(TOverloadBase)
  public
    function Equals(Other: TOverloadBase): Boolean; overload; override;
    function Equals(Obj: TObject): Boolean; overload; override;
  end;

  TStringHash = class
  public
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: string; override;
  end;

function TOnlyEquals.Equals(Obj: TObject): Boolean;
begin
  Result := False;
end;

function TOnlyHash.GetHashCode: Int64;
begin
  Result := 0;
end;

function TBoth.Equals(Obj: TObject): Boolean;
begin
  Result := False;
end;

function TBoth.GetHashCode: Int64;
begin
  Result := 0;
end;

function TChildEquals.Equals(Obj: TObject): Boolean;
begin
  Result := True;
end;

function TNoOverride.Equals(Obj: TObject): Boolean;
begin
  Result := False;
end;

function TNoOverride.GetHashCode: Int64;
begin
  Result := 0;
end;

function TOtherDialect.Equals(Obj: TObject): Boolean;
begin
  Result := False;
end;

function TOtherDialect.GetHashCode: Integer;
begin
  Result := 0;
end;

function TOverloadBase.Equals(Other: TOverloadBase): Boolean;
begin
  Result := False;
end;

function TOverloads.Equals(Other: TOverloadBase): Boolean;
begin
  Result := False;
end;

function TOverloads.Equals(Obj: TObject): Boolean;
begin
  Result := False;
end;

function TStringHash.Equals(Obj: TObject): Boolean;
begin
  Result := False;
end;

function TStringHash.GetHashCode: string;
begin
  Result := '';
end;

begin
end.
