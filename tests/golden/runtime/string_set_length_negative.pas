program StringSetLengthNegative;

// A negative length in `SetLength` of a string is an ERangeError like a dynamic array's
// (types.md chapter 7). Growing a string fills it with #0.

var
  S: string;
  N: Integer;
begin
  S := 'ab';
  SetLength(S, 4);
  WriteLn(Length(S), ' ', Ord(S[3]), ' ', Ord(S[4]));
  N := -2;
  SetLength(S, N);
end.
