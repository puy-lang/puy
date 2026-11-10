program DumpOverload;

// Overloads get distinct full names in declaration order (`Name`, `Name#2`; semantics.md 4.7),
// and a call that leaves out arguments passes the default values (4.8).

function Twice(N: Integer): Integer; overload;
begin
  Result := N * 2;
end;

function Twice(const S: string; Sep: string = '-'): string; overload;
begin
  Result := S + Sep + S;
end;

begin
  WriteLn(Twice(4));
  WriteLn(Twice('ab'));
end.
