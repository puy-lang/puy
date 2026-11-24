// A routine whose declarations start with a constructor's body reads it as part of its body
// (grammar.md 1.3, types.md 1.1), as for `procedure` / `function`: a second body of a global
// class's constructor written there is one E0701, not a routine without a body (E0411) followed
// by a chain of syntax errors.
program LocalTypeMethodBodyHeading;

type
  TG = class
    constructor Create;
  end;

constructor TG.Create;
begin
end;

procedure Run;
  constructor TG.Create;
  begin
  end;
begin
end;

begin
  Run;
end.
