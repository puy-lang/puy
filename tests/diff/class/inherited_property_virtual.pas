program InheritedPropertyVirtual;

// `inherited P` whose accessor methods are virtual and overridden by the child (objects.md 2.4):
// `inherited` only starts the search for `P` at the parent, and the accessors are called as for
// any property, through the instance's class (the child's `GetN` / `SetN`), as in Delphi.

type
  TA = class
  protected
    FN: Integer;
    function GetN: Integer; virtual;
    procedure SetN(X: Integer); virtual;
  public
    property N: Integer read GetN write SetN;
  end;

  TB = class(TA)
  protected
    function GetN: Integer; override;
    procedure SetN(X: Integer); override;
  public
    procedure Show;
  end;

function TA.GetN: Integer;
begin
  Result := FN;
end;

procedure TA.SetN(X: Integer);
begin
  WriteLn('TA.SetN ', X);
  FN := X;
end;

function TB.GetN: Integer;
begin
  Result := -1;
end;

procedure TB.SetN(X: Integer);
begin
  WriteLn('TB.SetN ', X);
end;

procedure TB.Show;
begin
  inherited N := 5;
  WriteLn(inherited N);
end;

var
  B: TB;
begin
  B := TB.Create;
  B.Show;
  B.Free;
end.
