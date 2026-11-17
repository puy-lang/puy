program DtorExceptions;

// Exceptions in the destruction steps (objects.md 8.5): after `BeforeDestruction` fails,
// `Destroy` still runs; when a destructor fails before its `inherited Destroy`, the parents'
// destructors it skipped run in turn, and the instance is destroyed in the end. Each new
// exception gets the one before it in its `Suppressed`, and the last one reaches the handler.
// The last `Free` leaves its exception unhandled: the report's trace goes from the destructor to
// the `Free` that started the destruction, without a frame of the destruction steps.

uses
  System.SysUtils;

type
  EStep = class(Exception);

  TBase = class
  public
    Name: string;
    destructor Destroy; override;
    procedure BeforeDestruction; override;
  end;

  TMiddle = class(TBase)
  public
    destructor Destroy; override;
  end;

  TLeaf = class(TMiddle)
  public
    destructor Destroy; override;
  end;

  TQuiet = class(TMiddle)
  public
    destructor Destroy; override;
  end;

destructor TBase.Destroy;
begin
  WriteLn('TBase.Destroy ', Name);
  inherited;
end;

procedure TBase.BeforeDestruction;
begin
  WriteLn('BeforeDestruction ', Name);
  if Name = 'before' then
    raise EStep.Create('BeforeDestruction failed');
end;

destructor TMiddle.Destroy;
begin
  WriteLn('TMiddle.Destroy ', Name);
  if Name <> 'quiet' then
    raise EStep.Create('TMiddle.Destroy failed');
  inherited;
end;

destructor TLeaf.Destroy;
begin
  WriteLn('TLeaf.Destroy ', Name);
  if Name = 'leaf' then
    raise EStep.Create('TLeaf.Destroy failed');
  inherited;
end;

destructor TQuiet.Destroy;
begin
  // Ends without `inherited Destroy`: the parents' destructors do not run
  WriteLn('TQuiet.Destroy ', Name);
end;

procedure Show(E: Exception; const Indent: string);
var
  I: Integer;
begin
  WriteLn(Indent, E.ClassName, ': ', E.Message);
  for I := 0 to Length(E.Suppressed) - 1 do
    Show(E.Suppressed[I], Indent + '  ');
end;

procedure Check(const Name: string; Leaf: Boolean);
var
  X: TBase;
begin
  if Leaf then
    X := TLeaf.Create
  else
    X := TMiddle.Create;
  X.Name := Name;
  try
    X.Free;
  except
    on E: Exception do
      Show(E, '');
  end;
  try
    WriteLn(X.Name);
  except
    on E: EAccessViolation do
      WriteLn('after: ', E.Message);
  end;
end;

procedure FreeUnhandled;
var
  X: TLeaf;
begin
  X := TLeaf.Create;
  X.Name := 'leaf';
  X.Free;
end;

var
  Q: TQuiet;
begin
  Check('before', False);
  Check('leaf', True);
  Check('middle', True);
  Q := TQuiet.Create;
  Q.Name := 'quiet';
  Q.Free;
  FreeUnhandled;
end.
