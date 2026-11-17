program Helpers;

uses
  System.SysUtils;

// Class and record helpers add methods and properties to an existing type
// without changing its declaration, including built-in types such as string
// and Integer. System has small built-in helpers too: ToString on numbers,
// Booleans, characters and enumerated types, and Length on strings and
// dynamic arrays.

type
  TSuit = (Clubs, Diamonds, Hearts, Spades);

  TStringTools = record helper for string
    function Reversed: string;
    function IsPalindrome: Boolean;
    function Repeated(Count: Integer): string;
    procedure Surround(const Left, Right: string);
  end;

  TIntegerTools = record helper for Integer
    function IsPrime: Boolean;
    function Digits: TArray<Integer>;
  end;

  TSuitTools = record helper for TSuit
    function Symbol: Char;
    function IsRed: Boolean;
  end;

  TPerson = class
  private
    FFirst, FLast: string;
  public
    constructor Create(const AFirst, ALast: string);
    property First: string read FFirst;
    property Last: string read FLast;
  end;

  // A class helper sees only the public members of TPerson.
  TPersonTools = class helper for TPerson
  private
    function GetInitials: string;
  public
    function FullName: string;
    property Initials: string read GetInitials;
  end;

  TPersonGreeting = class helper(TPersonTools) for TPerson
    function Greeting: string;
  end;

function TStringTools.Reversed: string;
begin
  Result := '';
  for var C in Self do
    Result := C + Result;
end;

function TStringTools.IsPalindrome: Boolean;
begin
  Result := LowerCase(Self) = LowerCase(Reversed);
end;

function TStringTools.Repeated(Count: Integer): string;
begin
  Result := '';
  for var I := 1 to Count do
    Result := Result + Self;
end;

// Self is the caller's variable, so assigning to it changes that variable.
procedure TStringTools.Surround(const Left, Right: string);
begin
  Self := Left + Self + Right;
end;

function TIntegerTools.IsPrime: Boolean;
begin
  if Self < 2 then
    Exit(False);
  var D := 2;
  while D * D <= Self do
  begin
    if Self mod D = 0 then
      Exit(False);
    Inc(D);
  end;
  Result := True;
end;

function TIntegerTools.Digits: TArray<Integer>;
begin
  Result := nil;
  var N := Abs(Self);
  repeat
    Insert(N mod 10, Result, 0);
    N := N div 10;
  until N = 0;
end;

function TSuitTools.Symbol: Char;
begin
  case Self of
    TSuit.Clubs: Result := 'C';
    TSuit.Diamonds: Result := 'D';
    TSuit.Hearts: Result := 'H';
  else
    Result := 'S';
  end;
end;

function TSuitTools.IsRed: Boolean;
begin
  Result := Self in [TSuit.Diamonds, TSuit.Hearts];
end;

constructor TPerson.Create(const AFirst, ALast: string);
begin
  inherited Create;
  FFirst := AFirst;
  FLast := ALast;
end;

function TPersonTools.GetInitials: string;
begin
  Result := First[1] + Last[1];
end;

function TPersonTools.FullName: string;
begin
  Result := First + ' ' + Last;
end;

function TPersonGreeting.Greeting: string;
begin
  Result := 'Hello, ' + FullName + ' (' + Initials + ')';
end;

function JoinDigits(const Digits: TArray<Integer>): string;
begin
  Result := '';
  for var D in Digits do
    Result := Result + '[' + D.ToString + ']';
end;

var
  Words: TArray<string>;
  Word, Dash: string;
  Count: Integer;
  Primes: TArray<Integer>;
  Person: TPerson;

begin
  WriteLn('string helper:');
  Words := ['Level', 'Puy', 'Racecar'];
  for var S in Words do
    WriteLn('  ', S, ' -> ', S.Reversed, ', palindrome: ', S.IsPalindrome.ToString);
  Dash := '-';
  WriteLn('  ', Dash.Repeated(3) + Words[1].Repeated(2));
  Word := 'core';
  Word.Surround('[', ']');
  WriteLn('  ', Word, ' has ', Word.Length, ' characters');

  WriteLn('integer helper:');
  Count := 30;
  for var N := 1 to Count do
    if N.IsPrime then
      Primes := Primes + [N];
  WriteLn('  ', Primes.Length, ' primes up to ', Count.ToString, ', the last is ',
    Primes[High(Primes)]);
  WriteLn('  digits of 2026: ', JoinDigits(2026.Digits));
  // The helper can be named explicitly; the receiver becomes the first argument.
  WriteLn('  TIntegerTools.IsPrime(97): ', TIntegerTools.IsPrime(97).ToString);

  WriteLn('enumerated type helper:');
  for var Suit := Low(TSuit) to High(TSuit) do
    WriteLn('  ', Suit.ToString:8, ' ', Suit.Symbol, ' red: ', Suit.IsRed.ToString);

  WriteLn('class helper:');
  Person := TPerson.Create('Ada', 'Lovelace');
  try
    WriteLn('  ', Person.FullName);
    WriteLn('  ', Person.Greeting);
  finally
    Person.Free;
  end;

  WriteLn('built-in helpers:');
  WriteLn('  ', 3.5.ToString, ' ', (Count > 0).ToString, ' ', 'x'.ToString, ' ', (-42).ToString);
end.
