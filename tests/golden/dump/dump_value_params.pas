program DumpValueParams;

// Managed value parameters in the ir (design/lifetime.md 3.3; M3-70): one the routine does not
// write borrows its argument like a `const` one (not in `.finally`); one it writes (assigned,
// passed to a `var` parameter, an element or a field written, a `for..in` loop variable, written
// by a nested routine) counts its argument first (`AddRef`, `AddRefObj` for a TObject one) and
// is released at the end; one captured by an anonymous method gets a counted copy in its cell.
// Writing an element of a dynamic array does not change the array (its elements are shared), so
// that parameter still borrows.

uses System.SysUtils;

type
  TPair = record
    Name: string;
    N: Integer;
  end;

  TNode = class(TRefObject)
  end;

  TProc = reference to procedure;

function ReadOnly(S: string; A: TArray<Integer>; N: TNode; O: TObject): Integer;
begin
  Result := Length(S) + Length(A);
  if (N <> nil) and (O <> nil) then
    Inc(Result);
end;

function Assigned_(S: string; O: TObject): string;
begin
  S := S + '!';
  O := nil;
  Result := S;
end;

procedure Append(var S: string);
begin
  S := S + '?';
end;

function PassedOn(S: string): string;
begin
  Append(S);
  Result := S;
end;

function FieldWritten(P: TPair): string;
begin
  P.Name := 'changed';
  Result := P.Name;
end;

procedure ElemWritten(A: TArray<Integer>);
begin
  A[0] := 9;
end;

function LoopVar(S: string; const Items: array of string): string;
begin
  Result := S;
  for S in Items do
    Result := Result + S;
end;

function Nested(S: string): string;

  procedure Change;
  begin
    S := 'nested';
  end;

begin
  Change;
  Result := S;
end;

function Captured(S: string): TProc;
begin
  Result := procedure
    begin
      WriteLn(S);
    end;
end;

var
  G: string;
  R: TPair;
  D: TArray<Integer>;
begin
  D := [1, 2];
  ElemWritten(D);
  WriteLn(D[0]);
  G := 'abc';
  R.Name := 'name';
  WriteLn(ReadOnly(G, [1, 2], TNode.Create, nil));
  WriteLn(Assigned_(G, nil), ' ', G);
  WriteLn(PassedOn(G), ' ', G);
  WriteLn(FieldWritten(R), ' ', R.Name);
  WriteLn(LoopVar(G, ['x', 'y']), ' ', G);
  WriteLn(Nested(G), ' ', G);
  Captured(G)();
end.
