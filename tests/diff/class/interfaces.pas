program Interfaces;

// Interfaces (objects.md 7.1-7.5): a class implements interfaces listed after its parent and
// their ancestors; a call through an interface reaches the instance's override of a virtual
// method, a private method, or the method a resolution clause names; interface properties read
// and write through the interface's methods; values of a descendant interface and of an
// implementing class are assigned to an interface; `=`, `<>` and `Assigned` compare instances.
// FPC needs `TInterfacedObject` for the methods of `IInterface` (`TRefObject` in Puy). Values of
// different interfaces and of a class are compared in golden/runtime/interface_identity (FPC
// compares the interface pointers, which differ).

uses System.SysUtils;

type
  IShape = interface
    function Area: Integer;
    function GetName: string;
    property Name: string read GetName;
  end;

  IResizable = interface(IShape)
    procedure Scale(Factor: Integer);
    function GetSize: Integer;
    procedure SetSize(Value: Integer);
    property Size: Integer read GetSize write SetSize;
  end;

  ILogger = interface
    procedure Log(const Msg: string);
    procedure Run;
  end;

  IRunner = interface
    procedure Run;
  end;

  TSquare = class(TInterfacedObject, IResizable, ILogger, IRunner)
  private
    FSide: Integer;
    function GetName: string;
    function GetSize: Integer;
    procedure SetSize(Value: Integer);
    procedure LogRun;
  public
    constructor Create(ASide: Integer);
    function Area: Integer; virtual;
    procedure Scale(Factor: Integer);
    procedure Log(const Msg: string);
    procedure ILogger.Run = LogRun;
    procedure Run;
  end;

  TCube = class(TSquare)
  public
    function Area: Integer; override;
  end;

constructor TSquare.Create(ASide: Integer);
begin
  inherited Create;
  FSide := ASide;
end;

function TSquare.GetName: string;
begin
  Result := 'square ' + IntToStr(FSide);
end;

function TSquare.GetSize: Integer;
begin
  Result := FSide;
end;

procedure TSquare.SetSize(Value: Integer);
begin
  WriteLn('SetSize ', Value);
  FSide := Value;
end;

procedure TSquare.LogRun;
begin
  WriteLn('ILogger.Run -> LogRun');
end;

function TSquare.Area: Integer;
begin
  Result := FSide * FSide;
end;

procedure TSquare.Scale(Factor: Integer);
begin
  FSide := FSide * Factor;
end;

procedure TSquare.Log(const Msg: string);
begin
  WriteLn('log: ', Msg);
end;

procedure TSquare.Run;
begin
  WriteLn('IRunner.Run -> Run');
end;

function TCube.Area: Integer;
begin
  Result := 6 * FSide * FSide;
end;

procedure Describe(const S: IShape);
begin
  WriteLn(S.Name, ': ', S.Area);
end;

function MakeShape(Side: Integer; Cube: Boolean): IResizable;
begin
  if Cube then
    Result := TCube.Create(Side)
  else
    Result := TSquare.Create(Side);
end;

var
  R, R2: IResizable;
  S: IShape;
  L: ILogger;
  Run: IRunner;
  Sq: TSquare;
begin
  R := MakeShape(3, False);
  Describe(R);
  R.Scale(2);
  Describe(R);
  R.Size := 5;
  WriteLn('size ', R.Size, ' name ', R.Name);

  // A virtual method implementing an interface method: the override is called
  S := MakeShape(2, True);
  Describe(S);

  // A descendant interface to an ancestor, and the same instance through both
  S := R;
  R2 := R;
  WriteLn(S = R, ' ', R2 = R, ' ', S <> nil, ' ', Assigned(R2));
  S := MakeShape(5, False);
  WriteLn(S = R, ' ', S <> R);

  // A class to the interfaces it implements, and a resolution clause
  Sq := TSquare.Create(7);
  L := Sq;
  Run := Sq;
  L.Log('hello');
  L.Run;
  Run.Run;
  S := nil;
  WriteLn(S = nil, ' ', Assigned(S));
end.
