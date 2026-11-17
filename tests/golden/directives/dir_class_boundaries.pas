program DirClassBoundaries;

// Directives between the members of a class, an interface and a helper are at a boundary
// (directives.md 3.1): after `class` and its parents, between visibility sections, between the
// fields of a `class var` section and before `end`. Inside a member they are not (W0005).

type
  IGreet = interface
    {$IFDEF NEVER}
    procedure Missing;
    {$ENDIF}
    function Greet: string;
  end;

  TBase = class(TRefObject, IGreet)
  {$IFDEF PUY}
    FName: string;
  {$ENDIF}
  class var
    {$IFDEF NEVER} Unused: Integer; {$ENDIF}
    Count: Integer;
  public
    {$IFDEF PUY}
    constructor Create(const AName: string);
    {$ENDIF}
    function Greet: string;
    procedure Show(N: Integer {$IFDEF PUY}; M: Integer{$ENDIF});
    Size: {$IFDEF PUY}Integer{$ENDIF};
  {$IFDEF NEVER}
    procedure Missing;
  {$ENDIF}
  end;

  TBaseHelper = class helper for TBase
    {$IFDEF PUY}
    function Loud: string;
    {$ENDIF}
  end;

constructor TBase.Create(const AName: string);
begin
  FName := AName;
  Inc(Count);
end;

function TBase.Greet: string;
begin
  Result := 'hello ' + FName;
end;

procedure TBase.Show(N: Integer; M: Integer);
begin
  WriteLn(N + M);
end;

function TBaseHelper.Loud: string;
begin
  Result := Greet + '!';
end;

var
  B: TBase;
  G: IGreet;
begin
  B := TBase.Create('puy');
  G := B;
  WriteLn(G.Greet);
  WriteLn(B.Loud);
  WriteLn(TBase.Count);
  B.Show(1, 2);
  B.Size := 4;
  WriteLn(B.Size);
end.
