program DumpCallArgs;

// Managed value arguments of method calls in the bytecode dump (design/lifetime.md 3.3; M3-70):
// for CallVirtual / CallVirtualClass / CallIntf / CallMethod alike, each is made (or a global
// copied, `AssignObj` for a TObject one) in a temporary of the caller and lent with `Move`, like
// a `const` argument; the callee owns none of them, so the function has no table for a call
// failing before its frame is made.

type
  IRun = interface
    procedure Run(S: string; N: Integer);
  end;

  TBase = class(TRefObject, IRun)
  public
    procedure Run(S: string; N: Integer); virtual;
    procedure Put(const C: string; O: TObject; S: string); virtual;
    procedure Plain(S: string);
    class procedure Make(S: string); virtual;
  end;

  TBaseClass = class of TBase;

procedure TBase.Run(S: string; N: Integer);
begin
  WriteLn(S, N);
end;

procedure TBase.Put(const C: string; O: TObject; S: string);
begin
  WriteLn(C, S);
end;

procedure TBase.Plain(S: string);
begin
  WriteLn(S);
end;

class procedure TBase.Make(S: string);
begin
  WriteLn(S);
end;

var
  B: TBase;
  I: IRun;
  C: TBaseClass;
  X: string;
begin
  X := 'x';
  B := TBase.Create;
  B.Run(X + '1', 1);
  B.Put(X, B, X + '2');
  B.Plain(X + '3');
  C := TBase;
  C.Make(X + '4');
  I := B;
  I.Run(X + '5', 5);
end.
