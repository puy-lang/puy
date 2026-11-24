// `native` is only written in the unit of an extension (E0770; `docs/spec/extensions.md` 2.1):
// on a routine or a method of a program it is an error, and the routine still needs a body.
program ExtNativeOutside;

type
  TThing = class
    procedure Run; native;
  end;

procedure Hello; native;

procedure TThing.Run;
begin
end;

procedure Outer;
  procedure Inner; native;
begin
end;

begin
end.
