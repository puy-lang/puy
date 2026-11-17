program ValueParamAlias;

// A string or dynamic array value parameter the routine does not write keeps the value it was
// called with while the routine changes the variable it came from: a global, a field of a global
// record, an element of a global array, or (through a `var` parameter) a local of the caller.
// A parameter the routine writes leaves the caller's variable as it was.

type
  TRec = record
    Name: string;
    Items: TArray<Integer>;
  end;

var
  G: string;
  GA: TArray<Integer>;
  R: TRec;
  List: array[0..1] of string;

procedure Fill(out A: TArray<Integer>; N, First: Integer);
var
  I: Integer;
begin
  SetLength(A, N);
  for I := 0 to N - 1 do
    A[I] := First + I;
end;

procedure ShowArr(const Title: string; const A: TArray<Integer>);
var
  I: Integer;
begin
  Write(Title, ':');
  for I := 0 to High(A) do
    Write(' ', A[I]);
  WriteLn;
end;

procedure ReadGlobal(S: string);
begin
  G := 'changed';
  WriteLn(S, ' ', G);
  G := G + '!';
  WriteLn(S, ' ', Length(S));
end;

procedure ReadGlobalArr(A: TArray<Integer>);
begin
  GA := nil;
  ShowArr('param', A);
  SetLength(GA, 1);
  GA[0] := 9;
  ShowArr('param', A);
end;

procedure ReadField(S: string; A: TArray<Integer>);
begin
  R.Name := R.Name + '?';
  Fill(R.Items, 1, 7);
  WriteLn(S, ' ', R.Name);
  ShowArr('param', A);
end;

procedure ReadElement(S: string);
begin
  List[0] := 'other';
  WriteLn(S, ' ', List[0]);
end;

procedure ReadThrough(S: string; var V: string);
begin
  V := V + ' and more';
  WriteLn(S, ' / ', V);
end;

procedure Written(S: string; A: TArray<Integer>);
begin
  S := S + ' (written)';
  SetLength(A, Length(A) + 1);
  A[High(A)] := 100;
  WriteLn(S);
  ShowArr('param', A);
end;

procedure RunLocal;
var
  L: string;
  LA: TArray<Integer>;
begin
  L := 'local';
  ReadThrough(L, L);
  WriteLn(L);
  Fill(LA, 2, 1);
  Written(L, LA);
  WriteLn(L);
  ShowArr('local', LA);
end;

begin
  G := 'global';
  ReadGlobal(G);
  WriteLn(G);
  Fill(GA, 3, 1);
  ReadGlobalArr(GA);
  ShowArr('global', GA);
  R.Name := 'field';
  Fill(R.Items, 2, 4);
  ReadField(R.Name, R.Items);
  WriteLn(R.Name);
  List[0] := 'element';
  ReadElement(List[0]);
  RunLocal;
end.
