program TypeSectionUnsupported;

// Types of later milestones and removed types in a `type` section (grammar.md chapter 3): one
// diagnostic each. A class is read with its body (a method is E3200 until M3-26), a forward-declared
// class is not a duplicate, and `class of` a forward-declared class is accepted (M3-29). A
// procedural type is read as a type (M3-45). The record after them is read normally.

type
  TFoo = class;
  TRef = class of TFoo;
  TFoo = class(TObject)
  public
    procedure Run;
  end;
  TIntf = interface
    procedure Go;
  end;
  TDisp = dispinterface
  end;
  TObj = object
    X: Integer;
  end;
  TColor = (Red, Green);
  TDigit = 0..9;
  TPtr = ^Integer;
  TProc = procedure(X: Integer);
  TPoint = record
    X: Integer;
  end;

var
  P: TPoint;
begin
  P := Default(TPoint);
  WriteLn(P.X);
end.
