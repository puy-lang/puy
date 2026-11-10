program OverloadNested;

// Resolving nested overloaded calls takes time in proportion to the nesting, not exponential
// in it (each argument is typed against every candidate; semantics.md 4.7).

function F(X: Integer): Integer; overload;
begin
  Result := X + 1;
end;

function F(X: Int64): Integer; overload;
begin
  Result := 0;
end;

function F(const S: string): Integer; overload;
begin
  Result := -1;
end;

begin
  WriteLn(F(F(F(F(F(F(F(F(F(F(F(F(F(F(F(F(F(F(F(F(1)))))))))))))))))))));
end.
