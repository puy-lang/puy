// Machine reports keep both streams, escape markup, and group tests by their full unit name.
unit A.First_test;

interface

implementation

[TestCase('a&<"''>')]
procedure Echo(const S: string);
begin
  WriteLn(S, #0, '終');
  Write(ErrOutput, 'err <>&', #13, #9);
end;

[Test]
procedure Assertion;
begin
  raise EAssertionFailed.Create('a < b & c');
end;

[Test, Skip('later "&"')]
procedure Skipped;
begin
end;

end.
