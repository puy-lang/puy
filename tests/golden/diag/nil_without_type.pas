program NilWithoutType;

// Diagnostics of a `nil` whose type no context gives (semantics.md 8.1): E0718 when
// written, nothing more next to an operand that already failed, and E0702 elsewhere.

type
  TFoo = class
  end;

var
  I: Integer;
  F: TFoo;
  S: string;

begin
  F := nil;
  WriteLn(nil);                         // E0718
  Write('a', (nil));                    // E0718
  if Undeclared = nil then;             // E0700 only
  if nil = Undeclared then;             // E0700 only
  if I = nil then;                      // E0702: expected `Integer`
  if nil <> S then;                     // E0702: expected `string`
  if nil = nil then;                    // E0702 once, at the first `nil`
  I := nil + 1;                         // E0702: expected `Integer`
  if nil then;                          // E0702
  var X := nil;                         // E0702
  if nil is TFoo then;                  // E0702: `is` needs an instance
  if Undeclared = (nil) then;           // E0700 only
  if I = (nil) then;                    // E0702: expected `Integer`
  if (nil) = I then;                    // E0702: expected `Integer`
  I := F + nil;                         // E0702 at `nil`: no `+` for a class
  I := nil + F;                         // E0702 at `nil`: no `+` for a class
  if F = nil then;
  if nil = F then;
  if F = (nil) then;
  if (nil) <> F then;
end.
