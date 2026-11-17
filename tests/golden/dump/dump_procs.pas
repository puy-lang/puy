program DumpProcs;

// The dumps of procedural types and method references (objects.md 15.5): the types written out
// with their parameters, `routine-ref` (`LoadInt` of the function index + 1), `method-ref` of a
// method, of a virtual method (`VirtualRef`) and of a class method (`ClassMethodRef`), the calls
// `call-indirect` / `call-closure` (`CallIndirect` / `CallClosure`, the frame before the
// arguments), `to-closure` (`RoutineRef`), `CallableAssigned` and `CallableEqual`, and a
// procedure called through a value as a statement (no destination).

type
  TCompare = function(A, B: Integer): Integer;
  TNotify = procedure(Sender: TObject) of object;
  TRun = reference to procedure;
  TFoo = class(TRefObject)
    procedure Handle(Sender: TObject); virtual;
    procedure Go;
    class procedure Hello;
  end;

procedure TFoo.Handle(Sender: TObject);
begin
end;

procedure TFoo.Go;
begin
end;

class procedure TFoo.Hello;
begin
end;

function Cmp(A, B: Integer): Integer;
begin
  Result := A - B;
end;

procedure Cmp2;
begin
end;

var
  C: TCompare;
  N: TNotify;
  R: TRun;
  O: TFoo;
begin
  C := Cmp;
  WriteLn(C(2, 1));
  O := TFoo.Create;
  N := O.Handle;
  N(O);
  R := O.Go;
  R();
  R := TFoo.Hello;
  R := Cmp2;
  WriteLn(Assigned(R), C = Cmp, N <> nil);
end.
