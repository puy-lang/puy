program Overload;

// Overloads are told apart by the conversion stage of each argument
// (semantics.md 4.7), and trailing arguments with default values can be
// left out (4.8).

procedure Show(X: Integer); overload;
begin
  WriteLn('Integer ', X);
end;

procedure Show(X: Int64); overload;
begin
  WriteLn('Int64 ', X);
end;

procedure Show(C: Char); overload;
begin
  WriteLn('Char ', C);
end;

procedure Show(const S: string); overload;
begin
  WriteLn('string ', S);
end;

procedure Show(X: Single); overload;
begin
  WriteLn('Single');
end;

procedure Show(X: Double); overload;
begin
  WriteLn('Double');
end;

procedure Line(A: Integer; B: Integer = 10; const S: string = 'tail');
begin
  WriteLn(A, ' ', B, ' ', S);
end;

function Twice(N: Integer = 21): Integer;
begin
  Result := N * 2;
end;

function Pick(N: Integer): string; overload; forward;
function Pick(const S: string): string; overload; forward;

procedure UsePick;
begin
  WriteLn(Pick(7));
  WriteLn(Pick('seven'));
end;

function Pick(N: Integer): string; overload;
begin
  Result := 'number';
end;

function Pick(const S: string): string; overload;
begin
  Result := 'text';
end;

function Sum(A: Integer; B: Integer): Integer; overload;
begin
  Result := A + B;
end;

function Sum(A: Integer; B: Integer; C: Integer): Integer; overload;
begin
  Result := A + B + C;
end;

var
  I: Integer;
  L: Int64;
  C: Char;
  S: string;
  F: Single;
  D: Double;
  N: Integer;

begin
  I := 3;
  L := 4;
  C := 'z';
  S := 'abc';
  F := 1.5;
  D := 2.5;
  Show(1);
  Show(I);
  Show(L);
  Show(9223372036854775807);
  Show('a');
  Show(C);
  Show('ab');
  Show(S);
  Show(F);
  Show(D);
  Line(1);
  Line(1, 2);
  Line(1, 2, 'end');
  N := Twice;
  WriteLn(N);
  WriteLn(Twice);
  WriteLn(Twice(5));
  Twice;
  UsePick;
  WriteLn(Sum(1, 2));
  WriteLn(Sum(1, 2, 3));
end.
