program InterfaceSharedCount;

// An interface's value and a class's value of one instance of a reference counted class share
// its count (objects.md 4.1, 7.3, 7.4): the instance lives while either holds it and is destroyed
// once, when the last reference goes. An instance of a class managed by hand is not counted
// through an interface and lives until `Free`.

type
  IGreeter = interface
    procedure Greet;
  end;

  TCounted = class(TRefObject, IGreeter)
  public
    destructor Destroy; override;
    procedure Greet;
  end;

  TManual = class(TObject, IGreeter)
  public
    destructor Destroy; override;
    procedure Greet;
  end;

destructor TCounted.Destroy;
begin
  WriteLn('destroy counted');
  inherited Destroy;
end;

procedure TCounted.Greet;
begin
  WriteLn('hello from counted');
end;

destructor TManual.Destroy;
begin
  WriteLn('destroy manual');
  inherited Destroy;
end;

procedure TManual.Greet;
begin
  WriteLn('hello from manual');
end;

procedure Counted;
var
  C: TCounted;
  G, H: IGreeter;
begin
  C := TCounted.Create;
  G := C;
  H := G;
  WriteLn('count: ', C.RefCount);
  C := nil;
  H := nil;
  WriteLn('one interface left');
  G := nil;
  WriteLn('none left');
end;

procedure Manual;
var
  M: TManual;
  G: IGreeter;
begin
  M := TManual.Create;
  G := M;
  G := nil;
  WriteLn('manual still alive: ', M.ClassName);
  M.Free;
end;

begin
  Counted;
  Manual;
  WriteLn('end');
end.
