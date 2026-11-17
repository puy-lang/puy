program MethodErrors;

// Diagnostics of methods (objects.md 2.3, 2.4, 3.1, 16): E0744 (an override that is not one, a
// narrowing override, `override; abstract;`, contradictory directives, an abstract method that is
// not virtual, calling an abstract method with `inherited`, a body of an abstract method), E0745
// (a final method, a sealed class, `sealed abstract`), E0742 with the abstract method, W0021 and
// W0026, E0743 for a method, E0700 (no such method; `inherited` outside a method or naming a
// method the parent does not have), E0704 (`inherited` alone in an expression), E0707 (`Self`),
// E0723 (no body, a heading that does not match), E3009 (`message`, and `Dispatch` /
// `DefaultHandler` of `TObject`) and E3010.

type
  TBase = class
  strict private
    procedure Hidden;
  public
    procedure Run(X: Integer); virtual;
    function Area: Double; virtual; abstract;
    procedure Locked; virtual; final;
    procedure Plain;
    procedure NoBody;
    function Twice(X: Integer): Integer;
  end;

  TChild = class(TBase)
  protected
    procedure Run(X: Integer); override;
  public
    procedure Missing; override;
    function Area: Double; override; abstract;
    procedure Locked; override;
    procedure Plain; reintroduce;
    procedure Fresh; reintroduce;
    procedure Run2; virtual; override;
    procedure Both; override; reintroduce;
    procedure NotVirtual; final;
    procedure NotClass; static;
    procedure OnlyAbstract; abstract;
    procedure Msg(var M: Integer); message 1;
    procedure Disp; dispid 3;
  end;

  TOther = class(TBase)
  public
    procedure Run(X: Integer);
    function Area: Double; virtual;
  end;

  TClosed = class sealed
  end;

  TFromClosed = class(TClosed)
  end;

  TBoth = class sealed abstract
  end;

procedure TBase.Hidden;
begin
end;

procedure TBase.Run(X: Integer);
begin
  Self := nil;
end;

function TBase.Area: Double;
begin
  Result := 0;
end;

procedure TBase.Locked;
begin
end;

procedure TBase.Plain;
begin
end;

function TBase.Twice(X: Integer; Y: Integer): Integer;
begin
  Result := X;
end;

function TBase.Twice(X: Integer): Integer;
begin
  Result := X * 2;
end;

procedure TBase.Nothing;
begin
end;

procedure TChild.Plain;
begin
end;

procedure TChild.Fresh;
begin
end;

procedure TChild.Run(X: Integer);
var
  I: Integer;
begin
  inherited Area;
  inherited Unknown;
  I := inherited;
end;

procedure TOther.Run(X: Integer);
begin
end;

function TOther.Area: Double;
begin
  Result := inherited Area;
end;

var
  B: TBase;
  N: Integer;
begin
  B := TBase.Create;
  B.Hidden;
  B.Absent;
  B.InstanceSize;
  inherited;
  B.Dispatch(N);
  B.DefaultHandler(N);
end.
