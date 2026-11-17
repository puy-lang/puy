program DistinctOps;

// The types of expressions with distinct types (types.md 1.3): a binary operator, `Abs` and `not`
// of a Boolean root yield the root type; a value read, unary `-` / `+` / `not` of an integer root
// (unless the root's result is wider than the root, as `-` of `Byte` is `Integer`),
// `Succ` / `Pred`, `Low` / `High`, `Default` and a conversion keep the distinct type, and `Inc`
// keeps the variable's. The overloads show which type each one has. A bound of a
// subrange or of a static array's index range may be a constant of a distinct type (as its root).

type
  TUserId = type Int64;
  TOrderId = type Int64;
  TLong = type TUserId;
  THtml = type string;
  TLetter = type Char;
  TFlag = type Boolean;
  TMeters = type Double;
  TMoney = type Currency;
  TBytes8 = type UTF8String;
  TSmall = type Byte;
  TInt = type Integer;
  TCard = type Cardinal;
  TSmallRange = TSmall(1)..TSmall(10);
  TLetters = TLetter('a')..TLetter('c');
  TSmallArray = array[TSmall(0)..TSmall(2)] of Integer;

procedure Show(Id: TUserId); overload;
begin
  WriteLn('user ', Id);
end;

procedure Show(N: Int64); overload;
begin
  WriteLn('number ', N);
end;

procedure Show(L: TLong); overload;
begin
  WriteLn('long ', L);
end;

procedure Flag(F: TFlag); overload;
begin
  WriteLn('flag ', F);
end;

procedure Flag(B: Boolean); overload;
begin
  WriteLn('boolean ', B);
end;

procedure Small(S: TSmall); overload;
begin
  WriteLn('small ', S);
end;

procedure Small(I: Integer); overload;
begin
  WriteLn('integer ', I);
end;

procedure Small(I: TInt); overload;
begin
  WriteLn('int ', I);
end;

procedure Small(C: TCard); overload;
begin
  WriteLn('card ', C);
end;

procedure Small(C: Cardinal); overload;
begin
  WriteLn('cardinal ', C);
end;

procedure Small(N: Int64); overload;
begin
  WriteLn('int64 ', N);
end;

procedure Text(H: THtml); overload;
begin
  WriteLn('html ', H);
end;

procedure Text(S: string); overload;
begin
  WriteLn('string ', S);
end;

begin
  var Id: TUserId := 42;
  var X := Id + 1;
  var Y := Id;
  Show(Id + 1);
  Show(Id + Id);
  Show(-Id);
  Show(+Id);
  Show(not Id);
  Show(Abs(Id));
  Show(-TUserId(5));
  Show(not TUserId(5));
  var Z := -Id;
  Show(Z);
  Show(X);
  Show(Y);
  Show(Succ(Id));
  Show(Pred(Id));
  Show(Low(TUserId));
  Show(High(TUserId));
  Show(Default(TUserId));
  Show(TUserId(7));
  Show(Int64(Id));
  Inc(Id);
  Show(Id);
  Id := Id + 1;
  Show(Id);
  var O: TOrderId := Id;
  WriteLn('order ', O);
  var L: TLong := Id;
  Show(L);
  Show(L + 1);
  Show(TUserId(L));

  var H: THtml := 'abc';
  Text(H);
  Text(H + '!');
  Text(Copy(H, 2, 2));
  WriteLn(H < 'abd', ' ', H[1], ' ', Low(H), ' ', High(H));
  var C: TLetter := 'x';
  Inc(C);
  WriteLn(C, ' ', Ord(C), ' ', Succ(C), ' ', C = 'y');
  var F: TFlag := False;
  Flag(not F);
  F := not F;
  if F and True then
    WriteLn('flag ', F);
  var M: TMeters := 2;
  WriteLn(M / 4:0:3, ' ', M + 0.25:0:2);
  var Money: TMoney := 1.25;
  WriteLn(Money * 2);
  var U: TBytes8 := 'utf8';
  WriteLn(U + '!', ' ', Length(U));
  var S: TSmall := 5;
  Small(-S);
  Small(+S);
  Small(not S);
  Small(-TSmall(5));
  Small(not TSmall(5));
  var I: TInt := 5;
  Small(-I);
  Small(not I);
  Small(-TInt(5));
  var Card: TCard := 5;
  Small(not Card);
  Small(-Card);
  Small(-TCard(5));
  var R: TSmallRange := 10;
  var Ls: TLetters := 'b';
  var A: TSmallArray := Default(TSmallArray);
  A[2] := 5;
  WriteLn(R, ' ', Low(TSmallRange), ' ', Ls, ' ', A[2], ' ', High(TSmallArray));
end.
