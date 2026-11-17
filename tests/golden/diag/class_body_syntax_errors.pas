program ClassBodySyntaxErrors;

// Syntax errors in class and interface bodies (objects.md chapter 1, 2.10, 7.1): `strict` not
// followed by `private` / `protected`, a word after `class` that starts no member, `forward` and
// `external` on a method of a class or an interface, an `automated` section (E3010), two GUIDs in
// an interface, and a class body whose `end` is missing (E0400 supplies it before `begin`; a `var`
// section or a routine there would still be read as members of the class).

type
  TA = class
  strict public
    X: Integer;
  end;

  TB = class
    class X: Integer;
  end;

  TC = class
    procedure Run; forward;
    procedure Go; external 'lib';
  automated
    Z: Integer;
  end;

  IFoo = interface
    ['{0F9A1B2C-3D4E-5F60-7182-93A4B5C6D7E8}']
    ['{0F9A1B2C-3D4E-5F60-7182-93A4B5C6D7E9}']
    procedure Run; forward;
  end;

  TD = class
    Y: Integer;

begin
  WriteLn(1);
end.
