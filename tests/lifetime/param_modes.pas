program ParamModes;

// Arguments by mode (lifetime.md 3.3): a global passed to a `const` parameter is copied into a
// temporary and lent from there (the callee could change the global); a local is lent as it is,
// unless the same call also passes it to a `var` parameter; an `out` argument is cleared before
// the call; a `var` parameter is written with `StoreRefRef`.

var
  G: string;

procedure Change(const S: string);
begin
  G := 'changed';
  WriteLn(S);
end;

procedure Fill(out S: string);
begin
  S := 'filled';
end;

procedure Append(var S: string);
begin
  S := S + '!';
end;

procedure Both(const A: string; var B: string);
begin
  B := 'new';
  WriteLn(A, ' ', B);
end;

procedure Run;
var
  L: string;
begin
  G := 'global';
  Change(G);
  WriteLn(G);
  L := 'local';
  Change(L);
  Both(L, L);
  Fill(L);
  WriteLn(L);
  Append(L);
  Append(G);
  WriteLn(L, ' ', G);
end;

begin
  Run;
end.
