program TypeSectionUnsupported;

// Types of later milestones and removed types in a `type` section (grammar.md chapter 3): one
// diagnostic each, a class body (with its `end`) is skipped as a whole, and a forward-declared
// class is not a duplicate. The record after them is read normally.

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
