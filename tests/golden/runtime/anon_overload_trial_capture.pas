program AnonOverloadTrialCapture;

// A routine nested in an anonymous method that changes a variable of the routine outside it,
// called from an anonymous method inside, in an argument that the choice among overloads
// analyzes once per trial (objects.md 9.6, 9.7): every trial takes the same routines, so the
// call reaches the one routine that captures the variable.

type
  TP = reference to procedure;
  TF0 = reference to function: Integer;

function Wrap(F: TP): string;
begin
  F();
  Result := 'wrapped';
end;

procedure Z(const S: string); overload;
begin
  WriteLn(S);
end;

procedure Z(F: TF0); overload;
begin
  WriteLn(F());
end;

procedure Run;
var
  K: Integer;
begin
  K := 5;
  Z(Wrap(procedure
    procedure Inner;
    begin
      Inc(K);
      WriteLn('inner ', K);
    end;
    begin
      Inner;
      Z(Wrap(procedure begin Inner; end));
    end));
  WriteLn(K);
end;

begin
  Run;
end.
