program ProcValueErrors;

// Errors with procedural values (objects.md 9.2-9.5, 9.7, 9.10): what cannot be a procedural
// value (E0752: a built-in routine, a member of `TObject` that is not virtual, a constructor, a
// record's method, an interface's method, `inherited`, a nested routine for a procedural type;
// for an anonymous method type the note shows how to wrap it; a nested routine is a value of an
// anonymous method type), a routine or method whose parameters or family do not fit
// (E0702), conversions between the families (E0702), comparisons of values of different families
// or signatures and an ordering of values that cannot be called (E0703), a value with parameters
// called without arguments or with too many (E0705), a procedure value used as a value (E0725),
// `Invoke` (E0700), a value that cannot be called without arguments passed to `WriteLn` (E0718),
// and a method reference to a new instance of a class managed by hand (W0023; the reference
// counted one is in lifetime/method_ref_temporary).

type
  TAction = procedure;
  TEvent = procedure of object;
  TRun = reference to procedure;
  TIntFn = function(X: Integer): Integer;
  TStrFn = function(S: string): Integer;

  TPoint = record
    X: Integer;
    procedure Reset;
  end;

  IRunner = interface
    procedure Run;
  end;

  TBase = class(TRefObject)
    procedure Run; virtual;
  end;

  TChild = class(TBase)
    constructor Make;
    procedure Run; override;
  end;

  TPlain = class
    procedure Go;
  end;

procedure TPoint.Reset;
begin
  X := 0;
end;

procedure TBase.Run;
begin
end;

constructor TChild.Make;
begin
end;

procedure TChild.Run;
var
  E: TEvent;
begin
  E := inherited Run;
  WriteLn(Assigned(E));
end;

procedure TPlain.Go;
begin
end;

procedure Hello;
begin
end;

function Square(X: Integer): Integer;
begin
  Result := X * X;
end;

procedure Outer;
var
  A: TAction;
  R: TRun;

  procedure Inner;
  begin
  end;

begin
  A := Inner;
  R := Inner;
  WriteLn(Assigned(A), Assigned(R));
end;

var
  A: TAction;
  E: TEvent;
  R: TRun;
  F: TIntFn;
  S: TStrFn;
  P: TPoint;
  I: IRunner;
  C: TChild;
  N: Integer;
begin
  C := TChild.Create;
  A := WriteLn;
  R := WriteLn;
  E := C.Free;
  E := C.ClassName;
  A := TChild.Make;
  E := P.Reset;
  E := I.Run;
  S := Square;
  E := Hello;
  A := C.Run;
  E := R;
  A := R;
  E := A;
  if A = E then
    WriteLn;
  if F = S then
    WriteLn;
  if F < F then
    WriteLn;
  N := F;
  N := F(1, 2);
  N := A;
  R.Invoke;
  WriteLn(A, F);
  E := TPlain.Create.Go;
end.
