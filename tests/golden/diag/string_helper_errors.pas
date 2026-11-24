program StringHelperErrors;

// The diagnostics of TStringHelper's members (spec/stdlib/System/TStringHelper.md): a name it
// does not have is E0700 now that the helper has its members (the E3200 of the helpers to come
// stays for types whose helper has none yet, such as Currency); Chars and the methods cannot be
// assigned (E0707); its private getter is E0743; arguments no overload takes are E0735; and a
// user's helper declaring a member of the same name makes the call ambiguous (E0754).

type
  TMyText = record helper for string
    function Trim: string;
  end;

function TMyText.Trim: string;
begin
  Result := Self;
end;

var
  S: string;
begin
  S := 'a,b';
  WriteLn(S.Foo);                    // E0700
  S.Foo := 'x';                      // E0700
  WriteLn(TStringHelper.Foo(S));     // E0700
  S.Chars[0] := 'x';                 // E0707
  S.IndexOf := 1;                    // E0707
  WriteLn(S.GetChars(0));            // E0743
  WriteLn(S.IndexOf(1));             // E0735
  WriteLn(S.Substring(1, 2, 3));     // E0735
  WriteLn(S.Trim);                   // E0754
end.
