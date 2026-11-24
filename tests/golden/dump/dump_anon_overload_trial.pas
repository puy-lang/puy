program DumpAnonOverloadTrial;

uses System.SysUtils;

// Choosing among overloads analyzes an argument holding anonymous methods once per trial
// (objects.md 9.6): the anonymous methods keep the numbers of the order they are written in
// (`<anon2>` in the stack trace), and the routine table holds one routine per anonymous method,
// per nested routine and per method of a local type declared in one, with no `#2` names.

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
begin
  Z(Wrap(procedure
    type
      TLoc = class
        procedure M;
      end;
    procedure TLoc.M;
    begin
      WriteLn('method');
    end;
    procedure Inner;
    begin
      WriteLn('inner');
    end;
    var
      T: TLoc;
    begin
      Inner;
      T := TLoc.Create;
      T.M;
      T.Free;
      Z(Wrap(procedure begin WriteLn('deep'); end));
    end));
  Z(Wrap(procedure begin raise Exception.Create('second'); end));
end;

begin
  Z(Wrap(procedure begin WriteLn('main'); end));
  Run;
end.
