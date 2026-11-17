program ClassConstructors;

// Constructors and destructors (objects.md 3.1, 3.3, chapter 5): the arguments reach the
// constructor, `inherited Create(…)` runs the parent's constructor on the same instance, then
// `AfterConstruction` runs; `Free` and `Destroy` run `BeforeDestruction`, then `Destroy`, whose
// `inherited Destroy` (or `inherited` alone) runs the parent's. `Exit` in a constructor still goes
// on to `AfterConstruction`. An exception in a constructor destroys the instance (its
// destructors run) before the handler sees it, and the variable assigned keeps its old value.

uses
  System.SysUtils;

type
  EPartError = class(Exception);

  TShape = class
  public
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
    procedure AfterConstruction; override;
    procedure BeforeDestruction; override;
  end;

  TCircle = class(TShape)
  public
    Radius: Integer;
    constructor Create(const AName: string; ARadius: Integer); overload;
    constructor Create; overload;
    destructor Destroy; override;
  end;

  TPart = class
  public
    Id: Integer;
    constructor Create(AId: Integer);
    destructor Destroy; override;
  end;

  TMachine = class(TPart)
  public
    destructor Destroy; override;
  end;

constructor TShape.Create(const AName: string);
begin
  inherited Create;
  Name := AName;
  WriteLn('TShape.Create ', Name);
end;

destructor TShape.Destroy;
begin
  WriteLn('TShape.Destroy ', Name);
  inherited Destroy;
end;

procedure TShape.AfterConstruction;
begin
  inherited AfterConstruction;
  WriteLn('AfterConstruction ', Name);
end;

procedure TShape.BeforeDestruction;
begin
  WriteLn('BeforeDestruction ', Name);
  inherited;
end;

constructor TCircle.Create(const AName: string; ARadius: Integer);
begin
  inherited Create(AName);
  Radius := ARadius;
  WriteLn('TCircle.Create ', Radius);
  if Radius = 0 then
    Exit;
  WriteLn('radius set');
end;

constructor TCircle.Create;
begin
  inherited Create('unit');
  Radius := 1;
end;

destructor TCircle.Destroy;
begin
  WriteLn('TCircle.Destroy ', Radius);
  inherited;
end;

constructor TPart.Create(AId: Integer);
begin
  Id := AId;
  WriteLn('TPart.Create ', Id);
  if Id < 0 then
    raise EPartError.Create('negative id');
  WriteLn('not reached');
end;

destructor TPart.Destroy;
begin
  WriteLn('TPart.Destroy ', Id);
  inherited;
end;

destructor TMachine.Destroy;
begin
  WriteLn('TMachine.Destroy ', Id);
  inherited Destroy;
end;

var
  S: TShape;
  C: TCircle;
  P: TPart;
begin
  S := TShape.Create('square');
  WriteLn(S.Name);
  S.Free;
  C := TCircle.Create('circle', 5);
  WriteLn(C.Name, ' ', C.Radius);
  C.Destroy;
  C := TCircle.Create('dot', 0);
  C.Free;
  C := TCircle.Create;
  WriteLn(C.Name, ' ', C.Radius);
  C.Free;
  P := TPart.Create(1);
  try
    P := TMachine.Create(-2);
  except
    on E: Exception do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  WriteLn(P.Id);
  P.Free;
end.
