program OverloadProcValues;

// Overloads with a procedural parameter take an anonymous method by the
// candidate's type, and a procedural variable as a value rather than its
// result (semantics.md 4.7). FPC 3.2.2 has no anonymous methods; checked with
// dcc64.

uses
  System.SysUtils;

type
  TFn = reference to function(X: Integer): string;
  TIntFn = reference to function(X: Integer): Integer;
  TStrFn = reference to function(const S: string): Integer;
  TObjProc = procedure of object;
  TRefProc = reference to procedure;
  TF0 = reference to function: string;
  TP0 = function: string;

function GetS: string;
begin
  Result := 'gets';
end;

procedure P(const S: string); overload;
begin
  WriteLn('P string ', S);
end;

procedure P(F: TFn); overload;
begin
  WriteLn('P TFn ', F(7));
end;

procedure Q(F: TIntFn); overload;
begin
  WriteLn('Q TIntFn ', F(2));
end;

procedure Q(F: TStrFn); overload;
begin
  WriteLn('Q TStrFn ', F('abc'));
end;

procedure R(F: TObjProc); overload;
begin
  WriteLn('R of object');
end;

procedure R(F: TRefProc); overload;
begin
  WriteLn('R reference');
  F();
end;

procedure Z(const S: string); overload;
begin
  WriteLn('Z string ', S);
end;

procedure Z(F: TF0); overload;
begin
  WriteLn('Z TF0 ', F());
end;

procedure Y(const S: string); overload;
begin
  WriteLn('Y string ', S);
end;

procedure Y(F: TP0); overload;
begin
  WriteLn('Y TP0 ', F());
end;

var
  E: TFn;
  I: TIntFn;
  G: TF0;
  H: TP0;
begin
  P('x');
  // An anonymous method fits only the candidate of its type
  P(function(X: Integer): string begin Result := 'a' + IntToStr(X); end);
  Q(function(X: Integer): Integer begin Result := X * 10; end);
  Q(function(const S: string): Integer begin Result := Length(S); end);
  R(procedure begin WriteLn('called'); end);
  Z(function: string begin Result := 'anon'; end);
  // A procedural variable that needs arguments cannot be called bare
  E := function(X: Integer): string begin Result := 'e' + IntToStr(X); end;
  P(E);
  I := function(X: Integer): Integer begin Result := X + 1; end;
  Q(I);
  // One that can be called is still passed as a value when a candidate takes it
  G := function: string begin Result := 'g'; end;
  Z(G);
  H := GetS;
  Y(H);
end.
