// puy-test: memory-limit=64k
program ConcatOrder;

// A sum of three or more strings evaluates its operands left to right and joins them once all
// have run (ConcatStrN; bytecode.md 2.4, types.md 9.2): an operand that changes a variable read
// by an earlier one (a var argument, a nested routine, an anonymous method, a global) does not
// change what was read, and a result over the memory limit raises EOutOfMemory after every operand
// has run. A sum of more operands than one instruction takes (300 here) is joined in groups.

uses System.SysUtils;

type
  TStrFunc = reference to function: string;

var
  G: string;
  Count: Integer;
  Marked: Boolean;

function Bump(var V: string): string;
begin
  V := V + '+';
  Result := '|';
end;

function ChangeGlobal: string;
begin
  G := 'new';
  Result := '/';
end;

function Step: string;
begin
  Inc(Count);
  Result := IntToStr(Count mod 10);
end;

function Mark: string;
begin
  Marked := True;
  Result := '';
end;

procedure Order;
var
  A, S: string;
  F: TStrFunc;

  function Touch: string;
  begin
    A := 'touched';
    Result := '/';
  end;

begin
  A := 'a';
  S := A + Bump(A) + A;
  WriteLn(S, ' ', A);
  A := 'outer';
  S := A + Touch + A;
  WriteLn(S);
  A := 'cap';
  F := function: string
    begin
      A := 'anon';
      Result := '/';
    end;
  S := A + F() + A;
  WriteLn(S);
  G := 'old';
  S := G + ChangeGlobal + G;
  WriteLn(S, ' ', G);
end;

procedure Long;
var
  X, S: string;
begin
  X := 'x';
  Count := 0;
  S := X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X + Step + X
    + Step + X + Step + X + Step + X + Step + X + Step + X + Step;
  WriteLn(Length(S), ' ', Count, ' ', Copy(S, 1, 12), ' ', Copy(S, Length(S) - 5, 6));
end;

procedure OverLimit;
var
  S, T: string;
begin
  S := StringOfChar('a', 25000);
  Marked := False;
  try
    T := S + S + S + Mark;
    WriteLn('made ', Length(T));
  except
    on E: EOutOfMemory do
      WriteLn('EOutOfMemory, operands ran: ', Marked, ', ', Length(S));
  end;
end;

begin
  Order;
  Long;
  OverLimit;
end.
