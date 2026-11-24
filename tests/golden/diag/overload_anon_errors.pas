// Overloads that take an anonymous method or a procedural value (semantics.md 4.7): two
// anonymous method types of the same signature are ambiguous (E0734, as in Delphi), a method that
// fits no candidate lists the candidates (E0735, the argument written as an anonymous method),
// the body is checked only against the candidate chosen (or its own signature when the call does
// not resolve, so a variable it captures is read: no W0001), and in parentheses the method is
// called (E0729).

program OverloadAnonErrors;

type
  TA = reference to function(X: Integer): string;
  TB = reference to function(X: Integer): string;
  TC = reference to function(const S: string): Integer;

procedure Z(F: TA); overload;
begin
end;

procedure Z(F: TB); overload;
begin
end;

procedure W(const S: string); overload;
begin
end;

procedure W(F: TA); overload;
begin
end;

procedure Captures;
var
  K: Integer;
begin
  K := 1;
  Z(function(X: Integer): string begin if K = X then Result := '' else Result := Undeclared2; end);
end;

var
  C: TC;
begin
  Z(function(X: Integer): string begin Result := ''; end);
  W(function(X: Integer): Integer begin Result := X; end);
  W(function(X: Integer): string begin Result := Undeclared; end);
  W((function(X: Integer): string begin Result := ''; end));
  C := function(const S: string): Integer begin Result := 0; end;
  W(C);
end.
