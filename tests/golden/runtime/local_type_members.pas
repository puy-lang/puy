program LocalTypeMembers;

// Types declared in a routine (types.md 1.1, objects.md 2.3): the methods of a local class
// (including a constructor, a destructor, a class method and a property's read method) and of a
// local record use their own members, the constants and types of the routine that declares them,
// and routines that are not nested; values of the routine reach them through arguments and fields.
// Not in diff/: Delphi XE4 and FPC 3.2.2 refuse local classes and methods of local records.

uses
  System.SysUtils;

function Twice(N: Integer): Integer;
begin
  Result := N * 2;
end;

procedure Run(Start: Integer);
const
  Step = 10;
type
  TKind = (kOne, kTwo);
  TCounter = class
  private
    FValue: Integer;
    FKind: TKind;
  public
    function GetNext: Integer;
  public
    constructor Create(AValue: Integer);
    destructor Destroy; override;
    class function Base: Integer;
    procedure Add;
    function Doubled: Integer;
    property Value: Integer read FValue;
    property Next: Integer read GetNext;
  end;
  TPair = record
    A, B: Integer;
    function Sum: Integer;
  end;

  constructor TCounter.Create(AValue: Integer);
  begin
    inherited Create;
    FValue := AValue;
    FKind := TKind.kTwo;
  end;

  destructor TCounter.Destroy;
  begin
    WriteLn('destroy ', FValue);
    inherited Destroy;
  end;

  class function TCounter.Base: Integer;
  begin
    Result := Step * 2;
  end;

  function TCounter.GetNext: Integer;
  begin
    Result := FValue + 1;
  end;

  procedure TCounter.Add;
  begin
    FValue := FValue + Step;
  end;

  function TCounter.Doubled: Integer;
  begin
    Result := Twice(FValue);
    if FKind = TKind.kTwo then
      Result := Result + 1;
  end;

  function TPair.Sum: Integer;
  begin
    Result := A + B + Step;
  end;

var
  C: TCounter;
  P: TPair;
begin
  C := TCounter.Create(Start);
  try
    C.Add;
    WriteLn(C.Value);
    WriteLn(C.Doubled);
    WriteLn(C.Next);
    WriteLn(TCounter.Base);
  finally
    C.Free;
  end;
  P := Default(TPair);
  P.A := Start;
  P.B := 2;
  WriteLn(P.Sum);
end;

begin
  Run(3);
end.
