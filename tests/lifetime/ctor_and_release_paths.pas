program CtorAndReleasePaths;

// Creation and release paths without a case elsewhere (objects.md 3.1, 3.2, 3.3, 4.4): an
// exception in `AfterConstruction` destroys the instance; an exception while evaluating the
// constructor's arguments creates no instance; the managed fields are released in declaration
// order, the parent's first; `FreeAndNil` of a `TObject` holding a reference counted instance is
// EInvalidOperation and leaves the variable `nil`; a copy of a record with a `[weak]` field stays
// weak; an instance only a `[weak]` variable got from `Create` lives until the routine ends.

uses
  System.SysUtils;

type
  TLog = class(TRefObject)
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TAfter = class(TRefObject)
    destructor Destroy; override;
    procedure AfterConstruction; override;
  end;

  TArg = class(TRefObject)
    constructor Create(X: Integer);
    destructor Destroy; override;
  end;

  TParent = class(TRefObject)
    B1, B2: TLog;
  end;

  TChild = class(TParent)
    C1: TLog;
  end;

  TWeakRec = record
    [weak] Link: TLog;
  end;

constructor TLog.Create(const AName: string);
begin
  Name := AName;
end;

destructor TLog.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited Destroy;
end;

destructor TAfter.Destroy;
begin
  WriteLn('destroy after');
  inherited Destroy;
end;

procedure TAfter.AfterConstruction;
begin
  WriteLn('after construction');
  raise Exception.Create('in AfterConstruction');
end;

constructor TArg.Create(X: Integer);
begin
  WriteLn('create ', X);
end;

destructor TArg.Destroy;
begin
  WriteLn('destroy arg');
  inherited Destroy;
end;

function Fail: Integer;
begin
  raise Exception.Create('in an argument');
end;

procedure Fill(C: TChild);
begin
  C.C1 := TLog.Create('c1');
  C.B2 := TLog.Create('b2');
  C.B1 := TLog.Create('b1');
end;

procedure Fields;
var
  C: TChild;
begin
  C := TChild.Create;
  Fill(C);
  WriteLn('release child');
  C := nil;
end;

procedure WeakCopy;
var
  Strong: TLog;
  A: TWeakRec = (Link: nil);
  B: TWeakRec;
begin
  Strong := TLog.Create('weak target');
  A.Link := Strong;
  B := A;
  Strong := nil;
  WriteLn('copy is nil: ', B.Link = nil);
end;

procedure WeakCreate;
var
  [weak] W: TLog;
begin
  W := TLog.Create('weak created');
  WriteLn('still alive: ', W <> nil);
end;

var
  A: TAfter;
  G: TArg;
  O: TObject;
begin
  A := nil;
  try
    A := TAfter.Create;
  except
    on E: Exception do
      WriteLn('caught ', E.Message, ' ', Assigned(A));
  end;
  G := nil;
  try
    G := TArg.Create(Fail);
  except
    on E: Exception do
      WriteLn('caught ', E.Message, ' ', Assigned(G));
  end;
  Fields;
  O := TLog.Create('object');
  try
    FreeAndNil(O);
  except
    on E: EInvalidOperation do
      WriteLn(E.ClassName, ' ', Assigned(O));
  end;
  WeakCopy;
  WeakCreate;
  WriteLn('after WeakCreate');
end.
