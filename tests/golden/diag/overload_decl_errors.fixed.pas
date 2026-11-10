// Declaring overloads (semantics.md 4.7): every declaration of the name needs `overload`
// (E0701; the fix adds it), the parameter lists must differ (value and `const` count as the
// same), and a forward overload with no body is E0723.

program OverloadDeclErrors;

procedure A(X: Integer); overload;
begin
end;

procedure A(const S: string); overload;
begin
end;

procedure B(X: Integer); overload;
begin
end;

procedure B(const X: Integer); overload;
begin
end;

function C(X: Integer): Integer; overload;
begin
  Result := X;
end;

function C(X: Integer): string; overload;
begin
  Result := '';
end;

procedure E(X: Integer); overload; forward;
procedure E(const S: string); overload; forward;

procedure E(X: Integer); overload;
begin
end;

procedure G(X: Integer);
begin
end;

procedure G(X: Integer);
begin
end;

begin
end.
