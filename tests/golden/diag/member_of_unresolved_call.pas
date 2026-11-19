program MemberOfUnresolvedCall;

// A member written after a call that fails to resolve (no matching overload of the constructor,
// wrong number of arguments, a function named without its arguments, a procedure used as a
// value) reports only the call's error: the call is the error type, so `.Free` is not looked up
// in a built-in type's helpers (semantics.md chapter 3, 4.7)

type
  TF = class
    constructor Create(A: Integer); overload;
    constructor Create(const S: string); overload;
  end;

  TG = class
    constructor Create(A: Integer);
  end;

constructor TF.Create(A: Integer);
begin
end;

constructor TF.Create(const S: string);
begin
end;

constructor TG.Create(A: Integer);
begin
end;

function Make(A: Integer): TG;
begin
  Result := TG.Create(A);
end;

procedure Nothing;
begin
end;

begin
  TF.Create.Free;
  TG.Create.Free;
  Make.Free;
  Nothing.Free;
end.
